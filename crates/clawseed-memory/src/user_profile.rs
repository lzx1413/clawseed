//! SQLite-backed structured user profiles.

use async_trait::async_trait;
use chrono::Utc;
use clawseed_api::user_profile::{
    ProfileCategory, ProfileItem, ProfileItemInput, ProfileSource, ProfileStatus, UserProfile,
    UserProfileStore,
};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

const MAX_USER_ID_LEN: usize = 256;
const MAX_KEY_LEN: usize = 256;
const MAX_VALUE_BYTES: usize = 16 * 1024;

pub struct SqliteUserProfileStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteUserProfileStore {
    pub fn new(workspace_dir: &Path) -> anyhow::Result<Self> {
        let db_dir = workspace_dir.join("user_model");
        std::fs::create_dir_all(&db_dir)?;
        let conn = Connection::open(db_dir.join("profiles.db"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS user_profile_versions (
                user_id    TEXT PRIMARY KEY,
                version    INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS user_profile_items (
                id                  TEXT PRIMARY KEY,
                user_id             TEXT NOT NULL,
                key                 TEXT NOT NULL,
                value_json          TEXT NOT NULL,
                category            TEXT NOT NULL,
                confidence          REAL NOT NULL,
                source              TEXT NOT NULL,
                status              TEXT NOT NULL,
                evidence_session_id TEXT,
                expires_at          TEXT,
                created_at          TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                version             INTEGER NOT NULL,
                UNIQUE(user_id, key)
            );
            CREATE INDEX IF NOT EXISTS idx_profile_items_user
                ON user_profile_items(user_id);
            CREATE INDEX IF NOT EXISTS idx_profile_items_user_status
                ON user_profile_items(user_id, status);",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn validate_user_id(user_id: &str) -> anyhow::Result<()> {
        let len = user_id.len();
        if user_id.trim().is_empty() || len > MAX_USER_ID_LEN {
            anyhow::bail!("user_id must contain 1 to {MAX_USER_ID_LEN} bytes");
        }
        Ok(())
    }

    fn validate_input(input: &ProfileItemInput) -> anyhow::Result<String> {
        let key_len = input.key.len();
        if input.key.trim().is_empty() || key_len > MAX_KEY_LEN {
            anyhow::bail!("profile key must contain 1 to {MAX_KEY_LEN} bytes");
        }
        if !input
            .key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            anyhow::bail!("profile key may only contain letters, numbers, '.', '_' and '-'");
        }
        if !input.confidence.is_finite() || !(0.0..=1.0).contains(&input.confidence) {
            anyhow::bail!("profile confidence must be between 0 and 1");
        }
        if let Some(expires_at) = input.expires_at.as_deref() {
            chrono::DateTime::parse_from_rfc3339(expires_at)
                .map_err(|_| anyhow::anyhow!("expires_at must be an RFC 3339 timestamp"))?;
        }
        let value_json = serde_json::to_string(&input.value)?;
        if value_json.len() > MAX_VALUE_BYTES {
            anyhow::bail!("profile value exceeds {MAX_VALUE_BYTES} bytes");
        }
        Ok(value_json)
    }

    fn next_version(tx: &Transaction<'_>, user_id: &str, now: &str) -> anyhow::Result<u64> {
        let current = tx
            .query_row(
                "SELECT version FROM user_profile_versions WHERE user_id = ?1",
                params![user_id],
                |row| row.get::<_, u64>(0),
            )
            .optional()?
            .unwrap_or(0);
        let next = current.saturating_add(1);
        tx.execute(
            "INSERT INTO user_profile_versions(user_id, version, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET
                version = excluded.version,
                updated_at = excluded.updated_at",
            params![user_id, next, now],
        )?;
        Ok(next)
    }

    fn map_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProfileItem> {
        let value_json: String = row.get(3)?;
        let category: String = row.get(4)?;
        let source: String = row.get(6)?;
        let status: String = row.get(7)?;
        Ok(ProfileItem {
            id: row.get(0)?,
            user_id: row.get(1)?,
            key: row.get(2)?,
            value: serde_json::from_str(&value_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
            category: ProfileCategory::from_str(&category).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, e.into())
            })?,
            confidence: row.get(5)?,
            source: ProfileSource::from_str(&source).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, e.into())
            })?,
            status: ProfileStatus::from_str(&status).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, e.into())
            })?,
            evidence_session_id: row.get(8)?,
            expires_at: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
            version: row.get(12)?,
        })
    }

    fn select_item(
        tx: &Transaction<'_>,
        user_id: &str,
        item_id: &str,
    ) -> anyhow::Result<ProfileItem> {
        Ok(tx.query_row(
            "SELECT id, user_id, key, value_json, category, confidence, source, status,
                    evidence_session_id, expires_at, created_at, updated_at, version
             FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
            params![user_id, item_id],
            Self::map_item,
        )?)
    }
}

#[async_trait]
impl UserProfileStore for SqliteUserProfileStore {
    async fn load(&self, user_id: &str) -> anyhow::Result<UserProfile> {
        Self::validate_user_id(user_id)?;
        let conn = self.conn.lock();
        let version = conn
            .query_row(
                "SELECT version FROM user_profile_versions WHERE user_id = ?1",
                params![user_id],
                |row| row.get::<_, u64>(0),
            )
            .optional()?
            .unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT id, user_id, key, value_json, category, confidence, source, status,
                    evidence_session_id, expires_at, created_at, updated_at, version
             FROM user_profile_items
             WHERE user_id = ?1
             ORDER BY category ASC, key ASC",
        )?;
        let items = stmt
            .query_map(params![user_id], Self::map_item)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UserProfile {
            user_id: user_id.to_string(),
            version,
            items,
        })
    }

    async fn upsert(&self, user_id: &str, input: ProfileItemInput) -> anyhow::Result<ProfileItem> {
        Self::validate_user_id(user_id)?;
        let value_json = Self::validate_input(&input)?;
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let version = Self::next_version(&tx, user_id, &now)?;
        let existing = tx
            .query_row(
                "SELECT id, created_at FROM user_profile_items
                 WHERE user_id = ?1 AND key = ?2",
                params![user_id, input.key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let (id, created_at) =
            existing.unwrap_or_else(|| (Uuid::new_v4().to_string(), now.clone()));
        tx.execute(
            "INSERT INTO user_profile_items(
                id, user_id, key, value_json, category, confidence, source, status,
                evidence_session_id, expires_at, created_at, updated_at, version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(user_id, key) DO UPDATE SET
                value_json = excluded.value_json,
                category = excluded.category,
                confidence = excluded.confidence,
                source = excluded.source,
                status = excluded.status,
                evidence_session_id = excluded.evidence_session_id,
                expires_at = excluded.expires_at,
                updated_at = excluded.updated_at,
                version = excluded.version",
            params![
                id,
                user_id,
                input.key,
                value_json,
                input.category.to_string(),
                input.confidence,
                input.source.to_string(),
                input.status.to_string(),
                input.evidence_session_id,
                input.expires_at,
                created_at,
                now,
                version,
            ],
        )?;
        let item = Self::select_item(&tx, user_id, &id)?;
        tx.commit()?;
        Ok(item)
    }

    async fn delete_item(&self, user_id: &str, item_id: &str) -> anyhow::Result<bool> {
        Self::validate_user_id(user_id)?;
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
            params![user_id, item_id],
        )?;
        if deleted > 0 {
            Self::next_version(&tx, user_id, &now)?;
        }
        tx.commit()?;
        Ok(deleted > 0)
    }

    async fn clear(&self, user_id: &str) -> anyhow::Result<usize> {
        Self::validate_user_id(user_id)?;
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM user_profile_items WHERE user_id = ?1",
            params![user_id],
        )?;
        if deleted > 0 {
            Self::next_version(&tx, user_id, &now)?;
        }
        tx.commit()?;
        Ok(deleted)
    }

    async fn health_check(&self) -> bool {
        self.conn
            .lock()
            .query_row("SELECT 1", [], |_| Ok(()))
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(key: &str, value: serde_json::Value) -> ProfileItemInput {
        ProfileItemInput {
            key: key.into(),
            value,
            category: ProfileCategory::Preference,
            confidence: 1.0,
            source: ProfileSource::Explicit,
            status: ProfileStatus::Active,
            evidence_session_id: Some("session-1".into()),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn isolates_users_and_versions_updates() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();

        let first = store
            .upsert(
                "alice",
                input("response.style", serde_json::json!("concise")),
            )
            .await
            .unwrap();
        store
            .upsert(
                "bob",
                input("response.style", serde_json::json!("detailed")),
            )
            .await
            .unwrap();
        let updated = store
            .upsert(
                "alice",
                input("response.style", serde_json::json!("balanced")),
            )
            .await
            .unwrap();

        let alice = store.load("alice").await.unwrap();
        let bob = store.load("bob").await.unwrap();
        assert_eq!(alice.version, 2);
        assert_eq!(bob.version, 1);
        assert_eq!(alice.items.len(), 1);
        assert_eq!(bob.items.len(), 1);
        assert_eq!(first.id, updated.id);
        assert_eq!(alice.items[0].value, serde_json::json!("balanced"));
        assert_eq!(bob.items[0].value, serde_json::json!("detailed"));
    }

    #[tokio::test]
    async fn delete_and_clear_are_user_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        let alice = store
            .upsert("alice", input("language", serde_json::json!("zh-CN")))
            .await
            .unwrap();
        store
            .upsert("bob", input("language", serde_json::json!("en-US")))
            .await
            .unwrap();

        assert!(!store.delete_item("bob", &alice.id).await.unwrap());
        assert!(store.delete_item("alice", &alice.id).await.unwrap());
        assert_eq!(store.clear("bob").await.unwrap(), 1);
        assert!(store.load("alice").await.unwrap().items.is_empty());
        assert!(store.load("bob").await.unwrap().items.is_empty());
    }

    #[tokio::test]
    async fn rejects_invalid_profile_values() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        let mut invalid = input("language", serde_json::json!("zh-CN"));
        invalid.confidence = 1.1;
        assert!(store.upsert("owner", invalid).await.is_err());
    }
}
