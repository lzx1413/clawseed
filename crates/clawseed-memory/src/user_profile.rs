//! SQLite-backed structured user profiles.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use clawseed_api::user_profile::{
    InferenceWriteResult, ProfileCategory, ProfileChangeAction, ProfileChangePlan,
    ProfileImportResult, ProfileImportStrategy, ProfileItem, ProfileItemInput, ProfileKeyRegistry,
    ProfileMutationResult, ProfileSearchQuery, ProfileSource, ProfileStatus, UserProfile,
    UserProfileStore,
};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

const MAX_USER_ID_LEN: usize = 256;
const MAX_KEY_LEN: usize = 256;
const MAX_VALUE_BYTES: usize = 16 * 1024;

pub struct SqliteUserProfileStore {
    conn: Arc<Mutex<Connection>>,
    max_active_items_per_category: usize,
    min_observations_for_implicit_fact: usize,
    undo_retention_hours: u64,
}

impl SqliteUserProfileStore {
    pub fn new(workspace_dir: &Path) -> anyhow::Result<Self> {
        Self::with_governance(workspace_dir, 20, 2, 24)
    }

    pub fn with_governance(
        workspace_dir: &Path,
        max_active_items_per_category: usize,
        min_observations_for_implicit_fact: usize,
        undo_retention_hours: u64,
    ) -> anyhow::Result<Self> {
        let db_dir = workspace_dir.join("user_model");
        std::fs::create_dir_all(&db_dir)?;
        let mut conn = Connection::open(db_dir.join("profiles.db"))?;
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
                ON user_profile_items(user_id, status);
            CREATE TABLE IF NOT EXISTS user_profile_observations (
                id          TEXT PRIMARY KEY,
                user_id     TEXT NOT NULL,
                key         TEXT NOT NULL,
                value_json  TEXT NOT NULL,
                session_id  TEXT NOT NULL,
                confidence  REAL NOT NULL,
                created_at  TEXT NOT NULL,
                UNIQUE(user_id, key, value_json, session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_profile_observations_lookup
                ON user_profile_observations(user_id, key, value_json);
            CREATE TABLE IF NOT EXISTS user_profile_change_plans (
                plan_id          TEXT PRIMARY KEY,
                user_id          TEXT NOT NULL,
                expected_version INTEGER NOT NULL,
                actions_json     TEXT NOT NULL,
                affected_json    TEXT NOT NULL,
                summary          TEXT NOT NULL,
                requires_confirmation INTEGER NOT NULL,
                created_at       TEXT NOT NULL,
                expires_at       TEXT NOT NULL,
                applied_at       TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_profile_plans_user
                ON user_profile_change_plans(user_id, expires_at);
            CREATE TABLE IF NOT EXISTS user_profile_change_log (
                operation_id          TEXT PRIMARY KEY,
                user_id               TEXT NOT NULL,
                profile_version_before INTEGER NOT NULL,
                profile_version_after  INTEGER NOT NULL,
                before_json           TEXT NOT NULL,
                after_json            TEXT NOT NULL,
                created_at            TEXT NOT NULL,
                expires_at            TEXT NOT NULL,
                undone_at             TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_profile_change_log_user
                ON user_profile_change_log(user_id, expires_at);
            CREATE TABLE IF NOT EXISTS user_profile_key_migrations (
                item_id       TEXT PRIMARY KEY,
                user_id       TEXT NOT NULL,
                original_key  TEXT NOT NULL,
                canonical_key TEXT NOT NULL,
                row_json      TEXT NOT NULL,
                migrated_at   TEXT NOT NULL
            );",
        )?;
        Self::migrate_profile_keys(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            max_active_items_per_category: max_active_items_per_category.clamp(1, 100),
            min_observations_for_implicit_fact: min_observations_for_implicit_fact.clamp(1, 10),
            undo_retention_hours: undo_retention_hours.clamp(1, 168),
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

    fn normalize_input(mut input: ProfileItemInput) -> anyhow::Result<ProfileItemInput> {
        input.key = ProfileKeyRegistry::normalize(&input.key, input.category)
            .ok_or_else(|| anyhow::anyhow!("profile key is not allowed for this category"))?;
        Ok(input)
    }

    fn migrate_profile_keys(conn: &mut Connection) -> anyhow::Result<()> {
        let items = {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, key, value_json, category, confidence, source, status,
                        evidence_session_id, expires_at, created_at, updated_at, version
                 FROM user_profile_items ORDER BY user_id, key",
            )?;
            stmt.query_map([], Self::map_item)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let migrations = items
            .into_iter()
            .filter_map(|item| {
                let canonical = ProfileKeyRegistry::normalize(&item.key, item.category)?;
                (canonical != item.key).then_some((item, canonical))
            })
            .collect::<Vec<_>>();
        if migrations.is_empty() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        let mut changed = HashSet::<(String, String)>::new();
        for (item, canonical) in migrations {
            tx.execute(
                "INSERT OR IGNORE INTO user_profile_key_migrations(
                     item_id, user_id, original_key, canonical_key, row_json, migrated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    item.id,
                    item.user_id,
                    item.key,
                    canonical,
                    serde_json::to_string(&item)?,
                    now,
                ],
            )?;
            let existing = tx
                .query_row(
                    "SELECT id, user_id, key, value_json, category, confidence, source, status,
                            evidence_session_id, expires_at, created_at, updated_at, version
                     FROM user_profile_items WHERE user_id = ?1 AND key = ?2",
                    params![item.user_id, canonical],
                    Self::map_item,
                )
                .optional()?;
            if let Some(existing) = existing {
                let item_rank = Self::profile_authority_rank(&item);
                let existing_rank = Self::profile_authority_rank(&existing);
                if item_rank > existing_rank {
                    tx.execute(
                        "DELETE FROM user_profile_items WHERE id = ?1",
                        params![existing.id],
                    )?;
                    tx.execute(
                        "UPDATE user_profile_items SET key = ?1 WHERE id = ?2",
                        params![canonical, item.id],
                    )?;
                } else {
                    tx.execute(
                        "DELETE FROM user_profile_items WHERE id = ?1",
                        params![item.id],
                    )?;
                }
            } else {
                tx.execute(
                    "UPDATE user_profile_items SET key = ?1 WHERE id = ?2",
                    params![canonical, item.id],
                )?;
            }
            changed.insert((item.user_id, canonical));
        }
        let mut user_versions = std::collections::HashMap::<String, u64>::new();
        for (user_id, canonical) in changed {
            let version = match user_versions.get(&user_id) {
                Some(version) => *version,
                None => {
                    let version = Self::next_version(&tx, &user_id, &now)?;
                    user_versions.insert(user_id.clone(), version);
                    version
                }
            };
            tx.execute(
                "UPDATE user_profile_items SET version = ?1, updated_at = ?2
                 WHERE user_id = ?3 AND key = ?4",
                params![version, now, user_id, canonical],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn profile_authority_rank(item: &ProfileItem) -> (u8, u8, String, u64) {
        let source = match item.source {
            ProfileSource::Explicit => 3,
            ProfileSource::Imported => 2,
            ProfileSource::Inferred => 1,
        };
        let status = match item.status {
            ProfileStatus::Rejected => 3,
            ProfileStatus::Active => 2,
            ProfileStatus::Superseded => 1,
        };
        #[allow(clippy::cast_sign_loss)]
        let confidence = (item.confidence.clamp(0.0, 1.0) * 1_000_000.0) as u64;
        (source, status, item.updated_at.clone(), confidence)
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

    fn current_version(tx: &Transaction<'_>, user_id: &str) -> anyhow::Result<u64> {
        Ok(tx
            .query_row(
                "SELECT version FROM user_profile_versions WHERE user_id = ?1",
                params![user_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    fn select_all_items(tx: &Transaction<'_>, user_id: &str) -> anyhow::Result<Vec<ProfileItem>> {
        let mut stmt = tx.prepare(
            "SELECT id, user_id, key, value_json, category, confidence, source, status,
                    evidence_session_id, expires_at, created_at, updated_at, version
             FROM user_profile_items WHERE user_id = ?1 ORDER BY category, key",
        )?;
        Ok(stmt
            .query_map(params![user_id], Self::map_item)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn upsert_in_transaction(
        tx: &Transaction<'_>,
        user_id: &str,
        input: ProfileItemInput,
        value_json: String,
        now: &str,
        version: u64,
    ) -> anyhow::Result<ProfileItem> {
        let existing = tx
            .query_row(
                "SELECT id, created_at FROM user_profile_items
                 WHERE user_id = ?1 AND key = ?2",
                params![user_id, input.key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let (id, created_at) =
            existing.unwrap_or_else(|| (Uuid::new_v4().to_string(), now.to_string()));
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
        Self::select_item(tx, user_id, &id)
    }
}

#[async_trait]
impl UserProfileStore for SqliteUserProfileStore {
    async fn load(&self, user_id: &str) -> anyhow::Result<UserProfile> {
        Self::validate_user_id(user_id)?;
        let user_id = user_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<UserProfile> {
            let conn = conn.lock();
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
                user_id,
                version,
                items,
            })
        })
        .await?
    }

    async fn upsert(&self, user_id: &str, input: ProfileItemInput) -> anyhow::Result<ProfileItem> {
        Self::validate_user_id(user_id)?;
        let input = Self::normalize_input(input)?;
        let value_json = Self::validate_input(&input)?;
        let user_id = user_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<ProfileItem> {
            let now = Utc::now().to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            let version = Self::next_version(&tx, &user_id, &now)?;
            let item =
                Self::upsert_in_transaction(&tx, &user_id, input, value_json, &now, version)?;
            tx.commit()?;
            Ok(item)
        })
        .await?
    }

    async fn upsert_inferred_if_allowed(
        &self,
        user_id: &str,
        input: ProfileItemInput,
    ) -> anyhow::Result<InferenceWriteResult> {
        Self::validate_user_id(user_id)?;
        let input = Self::normalize_input(input)?;
        if input.source != ProfileSource::Inferred {
            anyhow::bail!("conditional inference writes require source=inferred");
        }
        let value_json = Self::validate_input(&input)?;
        let user_id = user_id.to_string();
        let conn = self.conn.clone();
        let max_active_items_per_category = self.max_active_items_per_category;
        let min_observations_for_implicit_fact = self.min_observations_for_implicit_fact;

        tokio::task::spawn_blocking(move || -> anyhow::Result<InferenceWriteResult> {
            let now = Utc::now().to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            let existing = tx
                .query_row(
                    "SELECT id, user_id, key, value_json, category, confidence, source, status,
                            evidence_session_id, expires_at, created_at, updated_at, version
                     FROM user_profile_items WHERE user_id = ?1 AND key = ?2",
                    params![user_id, input.key],
                    Self::map_item,
                )
                .optional()?;

            if let Some(existing) = existing.as_ref() {
                let protected_source = matches!(
                    existing.source,
                    ProfileSource::Explicit | ProfileSource::Imported
                );
                let blocked = protected_source
                    || existing.status == ProfileStatus::Rejected
                    || existing.confidence > input.confidence;
                if blocked {
                    tx.commit()?;
                    return Ok(InferenceWriteResult::Blocked);
                }
                if existing.value == input.value
                    && existing.category == input.category
                    && existing.status == input.status
                {
                    tx.commit()?;
                    return Ok(InferenceWriteResult::Unchanged);
                }
            } else {
                let active_in_category: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM user_profile_items
                     WHERE user_id = ?1 AND category = ?2 AND status = 'active'",
                    params![user_id, input.category.to_string()],
                    |row| row.get(0),
                )?;
                if input.status == ProfileStatus::Active
                    && active_in_category >= max_active_items_per_category as i64
                {
                    tx.commit()?;
                    return Ok(InferenceWriteResult::Blocked);
                }
            }

            if existing.is_none() && input.confidence < 0.95 {
                let Some(session_id) = input.evidence_session_id.as_deref() else {
                    tx.commit()?;
                    return Ok(InferenceWriteResult::Blocked);
                };
                tx.execute(
                    "INSERT OR IGNORE INTO user_profile_observations(
                         id, user_id, key, value_json, session_id, confidence, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        Uuid::new_v4().to_string(),
                        user_id,
                        input.key,
                        value_json,
                        session_id,
                        input.confidence,
                        now
                    ],
                )?;
                let observations: i64 = tx.query_row(
                    "SELECT COUNT(DISTINCT session_id)
                     FROM user_profile_observations
                     WHERE user_id = ?1 AND key = ?2 AND value_json = ?3",
                    params![user_id, input.key, value_json],
                    |row| row.get(0),
                )?;
                if observations < min_observations_for_implicit_fact as i64 {
                    tx.commit()?;
                    return Ok(InferenceWriteResult::Blocked);
                }
            }

            let version = Self::next_version(&tx, &user_id, &now)?;
            let item =
                Self::upsert_in_transaction(&tx, &user_id, input, value_json, &now, version)?;
            tx.execute(
                "DELETE FROM user_profile_observations WHERE user_id = ?1 AND key = ?2",
                params![user_id, item.key],
            )?;
            tx.commit()?;
            Ok(InferenceWriteResult::Written(Box::new(item)))
        })
        .await?
    }

    async fn delete_item(&self, user_id: &str, item_id: &str) -> anyhow::Result<bool> {
        Self::validate_user_id(user_id)?;
        let user_id = user_id.to_string();
        let item_id = item_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
            let now = Utc::now().to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            let deleted = tx.execute(
                "DELETE FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
                params![user_id, item_id],
            )?;
            if deleted > 0 {
                Self::next_version(&tx, &user_id, &now)?;
            }
            tx.commit()?;
            Ok(deleted > 0)
        })
        .await?
    }

    async fn clear(&self, user_id: &str) -> anyhow::Result<usize> {
        Self::validate_user_id(user_id)?;
        let user_id = user_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
            let now = Utc::now().to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            let deleted = tx.execute(
                "DELETE FROM user_profile_items WHERE user_id = ?1",
                params![user_id],
            )?;
            if deleted > 0 {
                Self::next_version(&tx, &user_id, &now)?;
            }
            tx.commit()?;
            Ok(deleted)
        })
        .await?
    }

    async fn import_items(
        &self,
        user_id: &str,
        items: Vec<ProfileItemInput>,
        strategy: ProfileImportStrategy,
    ) -> anyhow::Result<ProfileImportResult> {
        Self::validate_user_id(user_id)?;
        let mut keys = HashSet::with_capacity(items.len());
        let mut validated = Vec::with_capacity(items.len());
        for input in items {
            let input = Self::normalize_input(input)?;
            if !keys.insert(input.key.clone()) {
                anyhow::bail!("profile import contains duplicate key: {}", input.key);
            }
            let value_json = Self::validate_input(&input)?;
            validated.push((input, value_json));
        }

        let user_id = user_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<ProfileImportResult> {
            let now = Utc::now().to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            let deleted = if strategy == ProfileImportStrategy::Replace {
                tx.execute(
                    "DELETE FROM user_profile_items WHERE user_id = ?1",
                    params![user_id],
                )?
            } else {
                0
            };
            let mut imported = 0;
            let mut skipped = 0;
            for (input, value_json) in validated {
                if strategy == ProfileImportStrategy::Append {
                    let exists = tx
                        .query_row(
                            "SELECT 1 FROM user_profile_items WHERE user_id = ?1 AND key = ?2",
                            params![user_id, input.key],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                    if exists {
                        skipped += 1;
                        continue;
                    }
                }
                let version = Self::next_version(&tx, &user_id, &now)?;
                Self::upsert_in_transaction(&tx, &user_id, input, value_json, &now, version)?;
                imported += 1;
            }
            if deleted > 0 && imported == 0 {
                Self::next_version(&tx, &user_id, &now)?;
            }
            tx.commit()?;
            Ok(ProfileImportResult { imported, skipped })
        })
        .await?
    }

    async fn search(
        &self,
        user_id: &str,
        query: ProfileSearchQuery,
    ) -> anyhow::Result<Vec<ProfileItem>> {
        let profile = self.load(user_id).await?;
        let text = query.text.map(|value| value.to_lowercase());
        Ok(profile
            .items
            .into_iter()
            .filter(|item| query.category.is_none_or(|value| item.category == value))
            .filter(|item| query.source.is_none_or(|value| item.source == value))
            .filter(|item| query.status.is_none_or(|value| item.status == value))
            .filter(|item| {
                query
                    .key
                    .as_deref()
                    .is_none_or(|value| item.key.contains(value))
            })
            .filter(|item| {
                text.as_deref().is_none_or(|value| {
                    item.key.to_lowercase().contains(value)
                        || item.value.to_string().to_lowercase().contains(value)
                })
            })
            .filter(|item| {
                query
                    .updated_since
                    .as_deref()
                    .is_none_or(|value| item.updated_at.as_str() >= value)
            })
            .filter(|item| {
                query
                    .updated_until
                    .as_deref()
                    .is_none_or(|value| item.updated_at.as_str() <= value)
            })
            .collect())
    }

    async fn create_change_plan(
        &self,
        user_id: &str,
        actions: Vec<ProfileChangeAction>,
        ttl_minutes: u64,
        max_affected: usize,
    ) -> anyhow::Result<ProfileChangePlan> {
        Self::validate_user_id(user_id)?;
        if actions.is_empty() {
            anyhow::bail!("profile change plan requires at least one action");
        }
        if actions.len() > max_affected {
            anyhow::bail!("profile change plan exceeds maximum affected items");
        }

        let profile = self.load(user_id).await?;
        let now = Utc::now();
        let mut normalized_actions = Vec::with_capacity(actions.len());
        let mut affected = Vec::new();
        for action in actions {
            match action {
                ProfileChangeAction::Set { input } => {
                    let input = Self::normalize_input(input)?;
                    Self::validate_input(&input)?;
                    if let Some(item) = profile.items.iter().find(|item| item.key == input.key) {
                        affected.push(item.clone());
                    }
                    normalized_actions.push(ProfileChangeAction::Set { input });
                }
                ProfileChangeAction::Reject { ref item_id }
                | ProfileChangeAction::Delete { ref item_id }
                | ProfileChangeAction::Rename { ref item_id, .. } => {
                    let item = profile
                        .items
                        .iter()
                        .find(|item| item.id == *item_id)
                        .ok_or_else(|| anyhow::anyhow!("profile item not found: {item_id}"))?;
                    if let ProfileChangeAction::Rename { ref new_key, .. } = action {
                        ProfileKeyRegistry::normalize(new_key, item.category).ok_or_else(|| {
                            anyhow::anyhow!("profile key is not allowed for this category")
                        })?;
                    }
                    affected.push(item.clone());
                    normalized_actions.push(action);
                }
                ProfileChangeAction::Merge {
                    item_ids,
                    target_key,
                } => {
                    if item_ids.len() < 2 {
                        anyhow::bail!("profile merge requires at least two item ids");
                    }
                    let mut unique = item_ids.clone();
                    unique.sort();
                    unique.dedup();
                    if unique.len() != item_ids.len() {
                        anyhow::bail!("profile merge contains duplicate item ids");
                    }
                    let merge_items = item_ids
                        .iter()
                        .map(|item_id| {
                            profile
                                .items
                                .iter()
                                .find(|item| item.id == *item_id)
                                .cloned()
                                .ok_or_else(|| anyhow::anyhow!("profile item not found: {item_id}"))
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    let category = merge_items[0].category;
                    if merge_items.iter().any(|item| item.category != category) {
                        anyhow::bail!("profile merge items must use the same category");
                    }
                    let target_key = ProfileKeyRegistry::normalize(&target_key, category)
                        .ok_or_else(|| {
                            anyhow::anyhow!("profile key is not allowed for this category")
                        })?;
                    affected.extend(merge_items);
                    if let Some(existing) = profile
                        .items
                        .iter()
                        .find(|item| item.key == target_key && !item_ids.contains(&item.id))
                    {
                        affected.push(existing.clone());
                    }
                    normalized_actions.push(ProfileChangeAction::Merge {
                        item_ids,
                        target_key,
                    });
                }
                ProfileChangeAction::DeleteExpired => {
                    affected.extend(
                        profile
                            .items
                            .iter()
                            .filter(|item| {
                                item.expires_at.as_deref().is_some_and(|expires_at| {
                                    chrono::DateTime::parse_from_rfc3339(expires_at)
                                        .is_ok_and(|expiry| expiry <= now)
                                })
                            })
                            .cloned(),
                    );
                    normalized_actions.push(ProfileChangeAction::DeleteExpired);
                }
            }
        }
        affected.sort_by(|left, right| left.id.cmp(&right.id));
        affected.dedup_by(|left, right| left.id == right.id);
        if affected.len().max(normalized_actions.len()) > max_affected {
            anyhow::bail!("profile change plan exceeds maximum affected items");
        }

        let requires_confirmation = normalized_actions.iter().any(|action| {
            !matches!(action, ProfileChangeAction::Set { .. }) || normalized_actions.len() > 1
        });
        let action_count = normalized_actions.len();
        let plan = ProfileChangePlan {
            plan_id: Uuid::new_v4().to_string(),
            expected_profile_version: profile.version,
            affected_items: affected,
            actions: normalized_actions,
            summary: format!("{action_count} profile change(s)"),
            requires_confirmation,
            expires_at: (now + Duration::minutes(ttl_minutes.clamp(1, 60) as i64)).to_rfc3339(),
        };
        let plan_for_db = plan.clone();
        let user_id = user_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock();
            conn.execute(
                "DELETE FROM user_profile_change_plans WHERE expires_at <= ?1 AND applied_at IS NULL",
                params![now.to_rfc3339()],
            )?;
            conn.execute(
                "INSERT INTO user_profile_change_plans(
                     plan_id, user_id, expected_version, actions_json, affected_json, summary,
                     requires_confirmation, created_at, expires_at, applied_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
                params![
                    plan_for_db.plan_id,
                    user_id,
                    plan_for_db.expected_profile_version,
                    serde_json::to_string(&plan_for_db.actions)?,
                    serde_json::to_string(&plan_for_db.affected_items)?,
                    plan_for_db.summary,
                    i64::from(plan_for_db.requires_confirmation),
                    now.to_rfc3339(),
                    plan_for_db.expires_at,
                ],
            )?;
            Ok(())
        })
        .await??;
        Ok(plan)
    }

    async fn apply_change_plan(
        &self,
        user_id: &str,
        plan_id: &str,
        max_affected: usize,
    ) -> anyhow::Result<ProfileMutationResult> {
        Self::validate_user_id(user_id)?;
        let user_id = user_id.to_string();
        let plan_id = plan_id.to_string();
        let conn = self.conn.clone();
        let undo_retention_hours = self.undo_retention_hours;
        tokio::task::spawn_blocking(move || -> anyhow::Result<ProfileMutationResult> {
            let now = Utc::now();
            let now_text = now.to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            let (expected_version, actions_json, expires_at, applied_at): (
                u64,
                String,
                String,
                Option<String>,
            ) = tx
                .query_row(
                    "SELECT expected_version, actions_json, expires_at, applied_at
                     FROM user_profile_change_plans WHERE plan_id = ?1 AND user_id = ?2",
                    params![plan_id, user_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("profile change plan not found"))?;
            if applied_at.is_some() {
                anyhow::bail!("profile change plan was already applied");
            }
            if expires_at.as_str() <= now_text.as_str() {
                anyhow::bail!("profile change plan expired");
            }
            let current_version = Self::current_version(&tx, &user_id)?;
            if current_version != expected_version {
                anyhow::bail!(
                    "profile version conflict: expected {expected_version}, current {current_version}"
                );
            }
            let actions: Vec<ProfileChangeAction> = serde_json::from_str(&actions_json)?;
            if actions.len() > max_affected {
                anyhow::bail!("profile change plan exceeds maximum affected items");
            }

            let before = Self::select_all_items(&tx, &user_id)?;
            let proposed_version = current_version.saturating_add(1);
            let mut affected = 0usize;
            let mut skipped = 0usize;
            for action in actions {
                match action {
                    ProfileChangeAction::Set { input } => {
                        let mut input = Self::normalize_input(input)?;
                        input.source = ProfileSource::Explicit;
                        input.confidence = 1.0;
                        let value_json = Self::validate_input(&input)?;
                        let unchanged = before.iter().any(|item| {
                            item.key == input.key
                                && item.value == input.value
                                && item.category == input.category
                                && item.status == input.status
                                && item.expires_at == input.expires_at
                        });
                        if unchanged {
                            skipped += 1;
                            continue;
                        }
                        Self::upsert_in_transaction(
                            &tx,
                            &user_id,
                            input,
                            value_json,
                            &now_text,
                            proposed_version,
                        )?;
                        affected += 1;
                    }
                    ProfileChangeAction::Reject { item_id } => {
                        let changed = tx.execute(
                            "UPDATE user_profile_items
                             SET status = 'rejected', updated_at = ?1, version = ?2
                             WHERE user_id = ?3 AND id = ?4 AND status != 'rejected'",
                            params![now_text, proposed_version, user_id, item_id],
                        )?;
                        affected += changed;
                        skipped += usize::from(changed == 0);
                    }
                    ProfileChangeAction::Delete { item_id } => {
                        let changed = tx.execute(
                            "DELETE FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
                            params![user_id, item_id],
                        )?;
                        affected += changed;
                        skipped += usize::from(changed == 0);
                    }
                    ProfileChangeAction::Rename { item_id, new_key } => {
                        let item = tx
                            .query_row(
                                "SELECT id, user_id, key, value_json, category, confidence,
                                        source, status, evidence_session_id, expires_at,
                                        created_at, updated_at, version
                                 FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
                                params![user_id, item_id],
                                Self::map_item,
                            )
                            .optional()?;
                        let Some(item) = item else {
                            skipped += 1;
                            continue;
                        };
                        let new_key = ProfileKeyRegistry::normalize(&new_key, item.category)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "profile key is not allowed for this category"
                                )
                            })?;
                        if new_key == item.key {
                            skipped += 1;
                            continue;
                        }
                        tx.execute(
                            "UPDATE user_profile_items
                             SET key = ?1, updated_at = ?2, version = ?3
                             WHERE user_id = ?4 AND id = ?5",
                            params![new_key, now_text, proposed_version, user_id, item_id],
                        )?;
                        affected += 1;
                    }
                    ProfileChangeAction::Merge {
                        item_ids,
                        target_key,
                    } => {
                        let mut merge_items = Vec::new();
                        for item_id in &item_ids {
                            if let Some(item) = tx
                                .query_row(
                                    "SELECT id, user_id, key, value_json, category, confidence,
                                            source, status, evidence_session_id, expires_at,
                                            created_at, updated_at, version
                                     FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
                                    params![user_id, item_id],
                                    Self::map_item,
                                )
                                .optional()?
                            {
                                merge_items.push(item);
                            }
                        }
                        let target_item = tx
                            .query_row(
                                "SELECT id, user_id, key, value_json, category, confidence,
                                        source, status, evidence_session_id, expires_at,
                                        created_at, updated_at, version
                                 FROM user_profile_items WHERE user_id = ?1 AND key = ?2",
                                params![user_id, target_key],
                                Self::map_item,
                            )
                            .optional()?;
                        if let Some(target_item) = target_item
                            && !merge_items.iter().any(|row| row.id == target_item.id)
                        {
                            merge_items.push(target_item);
                        }
                        if merge_items.len() < 2 {
                            skipped += 1;
                            continue;
                        }
                        merge_items.sort_by_key(Self::profile_authority_rank);
                        let winner = merge_items.pop().expect("merge has at least two items");
                        for item in &merge_items {
                            affected += tx.execute(
                                "DELETE FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
                                params![user_id, item.id],
                            )?;
                        }
                        if winner.key != target_key {
                            tx.execute(
                                "UPDATE user_profile_items SET key = ?1, updated_at = ?2, version = ?3
                                 WHERE user_id = ?4 AND id = ?5",
                                params![target_key, now_text, proposed_version, user_id, winner.id],
                            )?;
                            affected += 1;
                        }
                    }
                    ProfileChangeAction::DeleteExpired => {
                        let changed = tx.execute(
                            "DELETE FROM user_profile_items
                             WHERE user_id = ?1 AND expires_at IS NOT NULL AND expires_at <= ?2",
                            params![user_id, now_text],
                        )?;
                        affected += changed;
                        skipped += usize::from(changed == 0);
                    }
                }
                if affected > max_affected {
                    anyhow::bail!("profile change plan exceeds maximum affected items");
                }
            }

            let profile_version = if affected > 0 {
                Self::next_version(&tx, &user_id, &now_text)?
            } else {
                current_version
            };
            let after = Self::select_all_items(&tx, &user_id)?;
            let operation_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO user_profile_change_log(
                     operation_id, user_id, profile_version_before, profile_version_after,
                     before_json, after_json, created_at, expires_at, undone_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![
                    operation_id,
                    user_id,
                    current_version,
                    profile_version,
                    serde_json::to_string(&before)?,
                    serde_json::to_string(&after)?,
                    now_text,
                    (now + Duration::hours(undo_retention_hours as i64)).to_rfc3339(),
                ],
            )?;
            tx.execute(
                "UPDATE user_profile_change_plans SET applied_at = ?1 WHERE plan_id = ?2",
                params![now_text, plan_id],
            )?;
            tx.commit()?;
            Ok(ProfileMutationResult {
                profile_version,
                operation_id,
                affected,
                skipped,
            })
        })
        .await?
    }

    async fn undo_operation(
        &self,
        user_id: &str,
        operation_id: &str,
        retention_hours: u64,
    ) -> anyhow::Result<ProfileMutationResult> {
        Self::validate_user_id(user_id)?;
        let user_id = user_id.to_string();
        let operation_id = operation_id.to_string();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<ProfileMutationResult> {
            let now = Utc::now();
            let now_text = now.to_rfc3339();
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM user_profile_change_log WHERE expires_at <= ?1",
                params![now_text],
            )?;
            let (version_after, before_json, after_json, expires_at, undone_at): (
                u64,
                String,
                String,
                String,
                Option<String>,
            ) = tx
                .query_row(
                    "SELECT profile_version_after, before_json, after_json, expires_at, undone_at
                     FROM user_profile_change_log WHERE operation_id = ?1 AND user_id = ?2",
                    params![operation_id, user_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("profile operation not found"))?;
            if undone_at.is_some() {
                anyhow::bail!("profile operation was already undone");
            }
            if expires_at.as_str() <= now_text.as_str() {
                anyhow::bail!("profile operation undo expired");
            }
            let current_version = Self::current_version(&tx, &user_id)?;
            if current_version != version_after {
                anyhow::bail!(
                    "profile version conflict: expected {version_after}, current {current_version}"
                );
            }
            let before: Vec<ProfileItem> = serde_json::from_str(&before_json)?;
            let after: Vec<ProfileItem> = serde_json::from_str(&after_json)?;
            let before_by_id = before
                .into_iter()
                .map(|item| (item.id.clone(), item))
                .collect::<HashMap<_, _>>();
            let after_by_id = after
                .into_iter()
                .map(|item| (item.id.clone(), item))
                .collect::<HashMap<_, _>>();
            let mut changed_ids = before_by_id
                .keys()
                .chain(after_by_id.keys())
                .filter(|id| before_by_id.get(*id) != after_by_id.get(*id))
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            changed_ids.sort();

            let new_version = current_version.saturating_add(1);
            for item_id in &changed_ids {
                tx.execute(
                    "DELETE FROM user_profile_items WHERE user_id = ?1 AND id = ?2",
                    params![user_id, item_id],
                )?;
            }
            for item_id in &changed_ids {
                let Some(item) = before_by_id.get(item_id) else {
                    continue;
                };
                tx.execute(
                    "INSERT INTO user_profile_items(
                         id, user_id, key, value_json, category, confidence, source, status,
                         evidence_session_id, expires_at, created_at, updated_at, version
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        item.id,
                        user_id,
                        item.key,
                        serde_json::to_string(&item.value)?,
                        item.category.to_string(),
                        item.confidence,
                        item.source.to_string(),
                        item.status.to_string(),
                        item.evidence_session_id,
                        item.expires_at,
                        item.created_at,
                        now_text,
                        new_version,
                    ],
                )?;
            }
            tx.execute(
                "INSERT INTO user_profile_versions(user_id, version, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(user_id) DO UPDATE SET version = excluded.version,
                                                    updated_at = excluded.updated_at",
                params![user_id, new_version, now_text],
            )?;
            tx.execute(
                "UPDATE user_profile_change_log SET undone_at = ?1, expires_at = ?2
                 WHERE operation_id = ?3",
                params![
                    now_text,
                    (now + Duration::hours(retention_hours.min(168) as i64)).to_rfc3339(),
                    operation_id
                ],
            )?;
            tx.commit()?;
            Ok(ProfileMutationResult {
                profile_version: new_version,
                operation_id,
                affected: changed_ids.len(),
                skipped: 0,
            })
        })
        .await?
    }

    async fn health_check(&self) -> bool {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            conn.lock().query_row("SELECT 1", [], |_| Ok(())).is_ok()
        })
        .await
        .unwrap_or(false)
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

    #[tokio::test]
    async fn imports_profiles_with_replace_merge_and_append_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        store
            .upsert("owner", input("language", serde_json::json!("en-US")))
            .await
            .unwrap();

        let append = store
            .import_items(
                "owner",
                vec![
                    input("language", serde_json::json!("zh-CN")),
                    input("response.style", serde_json::json!("concise")),
                ],
                ProfileImportStrategy::Append,
            )
            .await
            .unwrap();
        assert_eq!(
            append,
            ProfileImportResult {
                imported: 1,
                skipped: 1
            }
        );
        assert_eq!(store.load("owner").await.unwrap().items.len(), 2);

        let mut imported = input("language", serde_json::json!("zh-CN"));
        imported.source = ProfileSource::Imported;
        imported.status = ProfileStatus::Rejected;
        let merge = store
            .import_items("owner", vec![imported], ProfileImportStrategy::Merge)
            .await
            .unwrap();
        assert_eq!(merge.imported, 1);
        let language = store
            .load("owner")
            .await
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.key == "preference.language")
            .unwrap();
        assert_eq!(language.value, serde_json::json!("zh-CN"));
        assert_eq!(language.source, ProfileSource::Imported);
        assert_eq!(language.status, ProfileStatus::Rejected);

        let replace = store
            .import_items(
                "owner",
                vec![input("expertise.rust", serde_json::json!(true))],
                ProfileImportStrategy::Replace,
            )
            .await
            .unwrap();
        assert_eq!(replace.imported, 1);
        let profile = store.load("owner").await.unwrap();
        assert_eq!(profile.items.len(), 1);
        assert_eq!(profile.items[0].key, "custom.preference.expertise.rust");
    }

    #[tokio::test]
    async fn profile_import_is_atomic_when_validation_fails() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        store
            .upsert("owner", input("language", serde_json::json!("en-US")))
            .await
            .unwrap();
        let result = store
            .import_items(
                "owner",
                vec![
                    input("valid.key", serde_json::json!(true)),
                    input("invalid/key", serde_json::json!(false)),
                ],
                ProfileImportStrategy::Replace,
            )
            .await;
        assert!(result.is_err());
        let profile = store.load("owner").await.unwrap();
        assert_eq!(profile.items.len(), 1);
        assert_eq!(profile.items[0].key, "preference.language");
    }

    #[tokio::test]
    async fn inferred_upsert_preserves_protected_rows_and_versions() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        store
            .upsert(
                "owner",
                input("preference.language", serde_json::json!("zh-CN")),
            )
            .await
            .unwrap();

        let mut inferred = input("preference.language", serde_json::json!("en-US"));
        inferred.source = ProfileSource::Inferred;
        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", inferred)
                .await
                .unwrap(),
            InferenceWriteResult::Blocked
        );
        assert_eq!(store.load("owner").await.unwrap().version, 1);

        let mut first = input("preference.response_style", serde_json::json!("concise"));
        first.source = ProfileSource::Inferred;
        first.confidence = 0.95;
        assert!(matches!(
            store
                .upsert_inferred_if_allowed("owner", first.clone())
                .await
                .unwrap(),
            InferenceWriteResult::Written(_)
        ));
        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", first)
                .await
                .unwrap(),
            InferenceWriteResult::Unchanged
        );
        assert_eq!(store.load("owner").await.unwrap().version, 2);

        let mut lower_confidence =
            input("preference.response_style", serde_json::json!("detailed"));
        lower_confidence.source = ProfileSource::Inferred;
        lower_confidence.confidence = 0.8;
        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", lower_confidence)
                .await
                .unwrap(),
            InferenceWriteResult::Blocked
        );
        assert_eq!(store.load("owner").await.unwrap().version, 2);

        let mut rejected = input("goal.primary", serde_json::json!("learn Rust"));
        rejected.source = ProfileSource::Inferred;
        rejected.status = ProfileStatus::Rejected;
        store.upsert("owner", rejected).await.unwrap();
        let mut retry = input("goal.primary", serde_json::json!("learn Go"));
        retry.source = ProfileSource::Inferred;
        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", retry)
                .await
                .unwrap(),
            InferenceWriteResult::Blocked
        );
        assert_eq!(store.load("owner").await.unwrap().version, 3);
    }

    #[tokio::test]
    async fn implicit_inference_requires_distinct_session_observations() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        let mut candidate = input("preference.output_format", serde_json::json!("markdown"));
        candidate.source = ProfileSource::Inferred;
        candidate.confidence = 0.85;

        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", candidate.clone())
                .await
                .unwrap(),
            InferenceWriteResult::Blocked
        );
        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", candidate.clone())
                .await
                .unwrap(),
            InferenceWriteResult::Blocked
        );
        assert_eq!(store.load("owner").await.unwrap().version, 0);

        candidate.evidence_session_id = Some("session-2".into());
        assert!(matches!(
            store
                .upsert_inferred_if_allowed("owner", candidate)
                .await
                .unwrap(),
            InferenceWriteResult::Written(_)
        ));
        assert_eq!(store.load("owner").await.unwrap().version, 1);
    }

    #[tokio::test]
    async fn change_plan_checks_version_applies_once_and_can_undo() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        let item = store
            .upsert(
                "owner",
                input("preference.language", serde_json::json!("zh-CN")),
            )
            .await
            .unwrap();
        let stale = store
            .create_change_plan(
                "owner",
                vec![ProfileChangeAction::Delete {
                    item_id: item.id.clone(),
                }],
                10,
                100,
            )
            .await
            .unwrap();
        let untouched = store
            .upsert(
                "owner",
                input("preference.output_format", serde_json::json!("markdown")),
            )
            .await
            .unwrap();
        let error = store
            .apply_change_plan("owner", &stale.plan_id, 100)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("version conflict"));

        let plan = store
            .create_change_plan(
                "owner",
                vec![ProfileChangeAction::Delete {
                    item_id: item.id.clone(),
                }],
                10,
                100,
            )
            .await
            .unwrap();
        assert!(plan.requires_confirmation);
        let applied = store
            .apply_change_plan("owner", &plan.plan_id, 100)
            .await
            .unwrap();
        assert_eq!(applied.affected, 1);
        assert_eq!(applied.profile_version, 3);
        assert!(
            store
                .apply_change_plan("owner", &plan.plan_id, 100)
                .await
                .unwrap_err()
                .to_string()
                .contains("already applied")
        );

        let undone = store
            .undo_operation("owner", &applied.operation_id, 24)
            .await
            .unwrap();
        assert_eq!(undone.affected, 1);
        assert_eq!(undone.profile_version, 4);
        let profile = store.load("owner").await.unwrap();
        assert!(profile.items.iter().any(|row| row.id == item.id));
        let untouched_after = profile
            .items
            .iter()
            .find(|row| row.id == untouched.id)
            .unwrap();
        assert_eq!(untouched_after.version, untouched.version);
        assert_eq!(untouched_after.updated_at, untouched.updated_at);
        assert!(
            store
                .undo_operation("owner", &applied.operation_id, 24)
                .await
                .unwrap_err()
                .to_string()
                .contains("already undone")
        );
    }

    #[tokio::test]
    async fn change_plan_ttl_and_search_are_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        let item = store
            .upsert(
                "owner",
                input("preference.language", serde_json::json!("zh-CN")),
            )
            .await
            .unwrap();
        let results = store
            .search(
                "owner",
                ProfileSearchQuery {
                    source: Some(ProfileSource::Explicit),
                    text: Some("zh-cn".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(store.load("owner").await.unwrap().version, 1);

        let plan = store
            .create_change_plan(
                "owner",
                vec![ProfileChangeAction::Delete { item_id: item.id }],
                10,
                100,
            )
            .await
            .unwrap();
        store
            .conn
            .lock()
            .execute(
                "UPDATE user_profile_change_plans SET expires_at = ?1 WHERE plan_id = ?2",
                params!["2000-01-01T00:00:00Z", plan.plan_id],
            )
            .unwrap();
        assert!(
            store
                .apply_change_plan("owner", &plan.plan_id, 100)
                .await
                .unwrap_err()
                .to_string()
                .contains("expired")
        );
    }

    #[tokio::test]
    async fn opening_existing_store_migrates_aliases_with_recovery_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        store
            .upsert(
                "owner",
                input("response.style", serde_json::json!("concise")),
            )
            .await
            .unwrap();
        drop(store);
        let conn = Connection::open(dir.path().join("user_model/profiles.db")).unwrap();
        conn.execute(
            "UPDATE user_profile_items SET key = 'response.style' WHERE user_id = 'owner'",
            [],
        )
        .unwrap();
        drop(conn);

        let reopened = SqliteUserProfileStore::new(dir.path()).unwrap();
        let profile = reopened.load("owner").await.unwrap();
        assert_eq!(profile.items[0].key, "preference.response_style");
        assert_eq!(profile.version, 2);
        let snapshots: i64 = reopened
            .conn
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM user_profile_key_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(snapshots, 1);
    }

    #[tokio::test]
    async fn governance_configuration_controls_category_cap_and_observations() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::with_governance(dir.path(), 1, 3, 12).unwrap();
        let mut first = input("preference.output_format", serde_json::json!("markdown"));
        first.source = ProfileSource::Inferred;
        first.confidence = 0.95;
        assert!(matches!(
            store
                .upsert_inferred_if_allowed("owner", first)
                .await
                .unwrap(),
            InferenceWriteResult::Written(_)
        ));
        let mut capped = input("preference.language", serde_json::json!("zh-CN"));
        capped.source = ProfileSource::Inferred;
        capped.confidence = 0.95;
        assert_eq!(
            store
                .upsert_inferred_if_allowed("owner", capped)
                .await
                .unwrap(),
            InferenceWriteResult::Blocked
        );

        let other = SqliteUserProfileStore::with_governance(dir.path(), 20, 3, 12).unwrap();
        let mut observed = input("preference.language", serde_json::json!("en-US"));
        observed.source = ProfileSource::Inferred;
        observed.confidence = 0.8;
        for session in ["one", "two"] {
            observed.evidence_session_id = Some(session.into());
            assert_eq!(
                other
                    .upsert_inferred_if_allowed("another", observed.clone())
                    .await
                    .unwrap(),
                InferenceWriteResult::Blocked
            );
        }
        observed.evidence_session_id = Some("three".into());
        assert!(matches!(
            other
                .upsert_inferred_if_allowed("another", observed)
                .await
                .unwrap(),
            InferenceWriteResult::Written(_)
        ));
    }

    #[tokio::test]
    async fn merge_plan_keeps_authoritative_item_and_removes_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteUserProfileStore::new(dir.path()).unwrap();
        let first = store
            .upsert(
                "owner",
                input("preference.topic_a", serde_json::json!("Rust")),
            )
            .await
            .unwrap();
        let mut inferred = input("preference.topic_b", serde_json::json!("rust"));
        inferred.source = ProfileSource::Inferred;
        inferred.confidence = 0.95;
        let second = store.upsert("owner", inferred).await.unwrap();
        let plan = store
            .create_change_plan(
                "owner",
                vec![ProfileChangeAction::Merge {
                    item_ids: vec![first.id.clone(), second.id],
                    target_key: "preference.code_language".into(),
                }],
                10,
                100,
            )
            .await
            .unwrap();
        let result = store
            .apply_change_plan("owner", &plan.plan_id, 100)
            .await
            .unwrap();
        assert!(result.affected >= 2);
        let profile = store.load("owner").await.unwrap();
        assert_eq!(profile.items.len(), 1);
        assert_eq!(profile.items[0].id, first.id);
        assert_eq!(profile.items[0].key, "preference.code_language");
    }
}
