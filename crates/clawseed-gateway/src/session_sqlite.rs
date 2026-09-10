//! SQLite-backed session persistence for the gateway.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clawseed_api::{provider::ChatMessage, tool::ToolPresentation};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};

use super::session_backend::{PersistedMessage, SessionBackend, SessionMetadata, SessionState};

pub struct SqliteSessionBackend {
    pub(super) conn: Arc<Mutex<Connection>>,
    pub(super) image_dir: std::path::PathBuf,
}

fn decode_attachments(
    json: String,
    column: usize,
) -> rusqlite::Result<Vec<clawseed_api::provider::ImageAttachment>> {
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn decode_files(
    json: String,
    column: usize,
) -> rusqlite::Result<Vec<clawseed_api::file_attachment::FileAttachment>> {
    serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

impl SqliteSessionBackend {
    pub fn new(workspace_dir: &std::path::Path) -> anyhow::Result<Self> {
        let db_dir = workspace_dir.join("gateway");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("sessions.db");

        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                session_key     TEXT PRIMARY KEY,
                name            TEXT,
                state           TEXT NOT NULL DEFAULT 'idle',
                turn_id         TEXT,
                turn_started_at TEXT,
                user_id         TEXT,
                created_at      TEXT NOT NULL,
                last_activity   TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_key TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                created_at  TEXT NOT NULL,
                FOREIGN KEY (session_key) REFERENCES sessions(session_key) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_key);

            CREATE TABLE IF NOT EXISTS image_attachments (
                id TEXT PRIMARY KEY,
                session_key TEXT NOT NULL,
                user_id TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );

            -- UI-only rich content, linked to the final assistant message.
            -- It is intentionally not part of messages.content, which is sent
            -- back to the LLM when a session is resumed.
            CREATE TABLE IF NOT EXISTS message_metrics (
                message_id INTEGER PRIMARY KEY,
                metrics_json TEXT NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS message_presentations (
                message_id      INTEGER PRIMARY KEY,
                schema_version  INTEGER NOT NULL,
                presentation_json TEXT NOT NULL,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
            );

            -- Persona↔session binding (旁路表，不碰 sessions 主表 schema).
            -- Write-once from the client's perspective; on resume the stored
            -- value is authoritative and ignores the ?persona= query param.
            CREATE TABLE IF NOT EXISTS session_personas (
                session_key TEXT PRIMARY KEY,
                persona     TEXT
            );",
        )?;

        // Existing databases retain their messages; old records have no images.
        let has_attachments = {
            let mut stmt = conn.prepare("PRAGMA table_info(messages)")?;
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == "attachments_json")
        };
        if !has_attachments {
            conn.execute_batch(
                "ALTER TABLE messages ADD COLUMN attachments_json TEXT NOT NULL DEFAULT '[]';",
            )?;
        }

        let has_files = {
            let mut stmt = conn.prepare("PRAGMA table_info(messages)")?;
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .any(|name| name == "files_json")
        };
        if !has_files {
            conn.execute_batch(
                "ALTER TABLE messages ADD COLUMN files_json TEXT NOT NULL DEFAULT '[]';",
            )?;
        }

        let has_user_id = {
            let mut stmt = conn.prepare("PRAGMA table_info(sessions)")?;
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .filter_map(Result::ok)
                .any(|name| name == "user_id")
        };
        if !has_user_id {
            conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN user_id TEXT;
                 CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);",
            )?;
        } else {
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);",
            )?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            image_dir: db_dir.join("images"),
        })
    }

    fn ensure_session(&self, conn: &Connection, session_key: &str) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO sessions (session_key, name, created_at, last_activity) VALUES (?1, ?2, ?3, ?4)",
            params![session_key, "新会话", now, now],
        )?;
        Ok(())
    }

    fn query_sessions_metadata(conn: &Connection, where_clause: &str) -> Vec<SessionMetadata> {
        let sql = format!(
            "SELECT s.session_key, s.created_at, s.last_activity, s.name,
                    sp.persona, s.user_id,
                    (SELECT COUNT(*) FROM messages m WHERE m.session_key = s.session_key) as msg_count
             FROM sessions s
             LEFT JOIN session_personas sp ON sp.session_key = s.session_key
             {where_clause}
             ORDER BY s.last_activity DESC"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, usize>(6)?,
            ))
        }) else {
            return Vec::new();
        };
        rows.filter_map(|r| {
            let (key, created_str, activity_str, name, persona, user_id, msg_count) = r.ok()?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .ok()?
                .with_timezone(&Utc);
            let last_activity = DateTime::parse_from_rfc3339(&activity_str)
                .ok()?
                .with_timezone(&Utc);
            Some(SessionMetadata {
                key,
                created_at,
                last_activity,
                message_count: msg_count,
                name,
                persona,
                user_id,
            })
        })
        .collect()
    }
}

#[async_trait]
impl SessionBackend for SqliteSessionBackend {
    fn upload_image(
        &self,
        session_key: &str,
        user_id: &str,
        bytes: &[u8],
    ) -> anyhow::Result<clawseed_api::provider::ImageAttachment> {
        self.store_image(session_key, user_id, bytes)
    }

    fn resolve_images(
        &self,
        session_key: &str,
        user_id: &str,
        ids: &[String],
    ) -> anyhow::Result<Vec<clawseed_api::provider::ImageAttachment>> {
        self.find_images(session_key, user_id, ids)
    }

    fn cleanup_images(&self) -> anyhow::Result<usize> {
        self.collect_images()
    }

    fn load(&self, session_key: &str) -> Vec<ChatMessage> {
        let conn = self.conn.lock();
        let mut stmt = match conn
            .prepare("SELECT role, content, attachments_json, files_json FROM messages WHERE session_key = ?1 ORDER BY id")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let Ok(rows) = stmt.query_map(params![session_key], |row| {
            Ok(ChatMessage {
                role: row.get(0)?,
                content: row.get(1)?,
                attachments: decode_attachments(row.get(2)?, 2)?,
                files: decode_files(row.get(3)?, 3)?,
                stable_prefix: None, // Not persisted; rebuilt by seed_history on resume
            })
        }) else {
            return Vec::new();
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    fn load_with_presentations(&self, session_key: &str) -> Vec<PersistedMessage> {
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT m.role, m.content, p.presentation_json, m.attachments_json, stats.metrics_json, m.files_json
             FROM messages m
             LEFT JOIN message_presentations p ON p.message_id = m.id
             LEFT JOIN message_metrics stats ON stats.message_id = m.id
             WHERE m.session_key = ?1
             ORDER BY m.id",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        let Ok(rows) = stmt.query_map(params![session_key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                decode_attachments(row.get(3)?, 3)?,
                row.get::<_, Option<String>>(4)?,
                decode_files(row.get(5)?, 5)?,
            ))
        }) else {
            return Vec::new();
        };

        rows.filter_map(|row| {
            let (role, content, presentation_json, attachments, metrics_json, files) = row.ok()?;
            let presentation = presentation_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<ToolPresentation>(json).ok());
            Some(PersistedMessage {
                role,
                content,
                attachments,
                files,
                presentation,
                metrics: metrics_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok()),
            })
        })
        .collect()
    }

    fn append(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        self.ensure_session(&conn, session_key)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO messages (session_key, role, content, created_at, attachments_json, files_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session_key, message.role, message.content, now, serde_json::to_string(&message.attachments)?, serde_json::to_string(&message.files)?],
        )?;
        conn.execute(
            "UPDATE sessions SET last_activity = ?1 WHERE session_key = ?2",
            params![now, session_key],
        )?;
        Ok(())
    }

    fn update_last(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE messages SET content = ?1 WHERE id = (
                SELECT id FROM messages WHERE session_key = ?2 AND role = ?3 ORDER BY id DESC LIMIT 1
            )",
            params![message.content, session_key, message.role],
        )?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET last_activity = ?1 WHERE session_key = ?2",
            params![now, session_key],
        )?;
        Ok(())
    }

    fn update_last_user(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE messages SET content = ?1 WHERE id = (
                SELECT id FROM messages WHERE session_key = ?2 AND role = 'user' ORDER BY id DESC LIMIT 1
            )",
            params![message.content, session_key],
        )?;
        Ok(())
    }

    fn set_last_assistant_metrics(
        &self,
        session_key: &str,
        metrics: &clawseed_api::provider::ResponseMetrics,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO message_metrics (message_id, metrics_json)
             SELECT id, ?2 FROM messages WHERE session_key = ?1 AND role = 'assistant'
             ORDER BY id DESC LIMIT 1
             ON CONFLICT(message_id) DO UPDATE SET metrics_json = excluded.metrics_json",
            params![session_key, serde_json::to_string(metrics)?],
        )?;
        Ok(())
    }

    fn set_last_assistant_presentation(
        &self,
        session_key: &str,
        presentation: &ToolPresentation,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        let message_id: i64 = conn.query_row(
            "SELECT id FROM messages
             WHERE session_key = ?1 AND role = 'assistant'
             ORDER BY id DESC LIMIT 1",
            params![session_key],
            |row| row.get(0),
        )?;
        let json = serde_json::to_string(presentation)?;
        conn.execute(
            "INSERT INTO message_presentations (message_id, schema_version, presentation_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(message_id) DO UPDATE SET
                 schema_version = excluded.schema_version,
                 presentation_json = excluded.presentation_json",
            params![message_id, presentation.version, json],
        )?;
        Ok(())
    }

    fn list_sessions(&self) -> Vec<String> {
        let conn = self.conn.lock();
        let mut stmt =
            match conn.prepare("SELECT session_key FROM sessions ORDER BY last_activity DESC") {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
        let Ok(rows) = stmt.query_map([], |row| row.get(0)) else {
            return Vec::new();
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    fn list_sessions_with_metadata(&self) -> Vec<SessionMetadata> {
        let conn = self.conn.lock();
        Self::query_sessions_metadata(&conn, "")
    }

    fn delete_session(&self, session_key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM sessions WHERE session_key = ?1",
            params![session_key],
        )?;
        Ok(deleted > 0)
    }

    fn set_session_name(&self, session_key: &str, name: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        self.ensure_session(&conn, session_key)?;
        conn.execute(
            "UPDATE sessions SET name = ?1 WHERE session_key = ?2",
            params![name, session_key],
        )?;
        Ok(())
    }

    fn get_session_name(&self, session_key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock();
        match conn.query_row(
            "SELECT name FROM sessions WHERE session_key = ?1",
            params![session_key],
            |row| row.get(0),
        ) {
            Ok(n) => Ok(n),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set_session_state(
        &self,
        session_key: &str,
        state: &str,
        turn_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        self.ensure_session(&conn, session_key)?;
        let turn_started_at = turn_id.map(|_| Utc::now().to_rfc3339());
        conn.execute(
            "UPDATE sessions SET state = ?1, turn_id = ?2, turn_started_at = ?3 WHERE session_key = ?4",
            params![state, turn_id, turn_started_at, session_key],
        )?;
        Ok(())
    }

    fn get_session_state(&self, session_key: &str) -> anyhow::Result<Option<SessionState>> {
        let conn = self.conn.lock();
        match conn.query_row(
            "SELECT state, turn_id, turn_started_at FROM sessions WHERE session_key = ?1",
            params![session_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        ) {
            Ok((state, turn_id, turn_started_str)) => {
                let turn_started_at = turn_started_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));
                Ok(Some(SessionState {
                    state,
                    turn_id,
                    turn_started_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn list_running_sessions(&self) -> Vec<SessionMetadata> {
        let conn = self.conn.lock();
        Self::query_sessions_metadata(&conn, "WHERE s.state = 'running'")
    }

    fn cleanup_stale(&self, ttl_hours: u32) -> anyhow::Result<usize> {
        let conn = self.conn.lock();
        let cutoff = (Utc::now() - chrono::Duration::hours(i64::from(ttl_hours))).to_rfc3339();
        let deleted = conn.execute(
            "DELETE FROM sessions WHERE last_activity < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    fn remove_last_assistant_turn(&self, session_key: &str) -> Option<String> {
        let conn = self.conn.lock();
        // Find the last user message
        let last_user: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, content FROM messages WHERE session_key = ?1 AND role = 'user' ORDER BY id DESC LIMIT 1",
                params![session_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        let (user_id, user_content) = last_user?;
        // Delete all messages after the last user message (the assistant turn)
        conn.execute(
            "DELETE FROM messages WHERE session_key = ?1 AND id > ?2",
            params![session_key, user_id],
        )
        .ok();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE sessions SET last_activity = ?1 WHERE session_key = ?2",
            params![now, session_key],
        )
        .ok();
        Some(user_content)
    }

    fn set_session_persona(&self, session_key: &str, persona: Option<&str>) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        match persona {
            Some(p) => {
                conn.execute(
                    "INSERT OR REPLACE INTO session_personas (session_key, persona) VALUES (?1, ?2)",
                    params![session_key, p],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM session_personas WHERE session_key = ?1",
                    params![session_key],
                )?;
            }
        }
        Ok(())
    }

    fn get_session_persona(&self, session_key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT persona FROM session_personas WHERE session_key = ?1")?;
        let mut rows = stmt.query(params![session_key])?;
        Ok(rows
            .next()?
            .and_then(|r| r.get::<_, Option<String>>(0).ok())
            .flatten())
    }

    fn bind_session_user(&self, session_key: &str, user_id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock();
        self.ensure_session(&conn, session_key)?;
        let existing = conn.query_row(
            "SELECT user_id FROM sessions WHERE session_key = ?1",
            params![session_key],
            |row| row.get::<_, Option<String>>(0),
        )?;
        if existing.as_deref().is_some_and(|owner| owner != user_id) {
            return Ok(false);
        }
        conn.execute(
            "UPDATE sessions SET user_id = ?1 WHERE session_key = ?2 AND user_id IS NULL",
            params![user_id, session_key],
        )?;
        Ok(true)
    }

    fn get_session_user(&self, session_key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock();
        Ok(conn
            .query_row(
                "SELECT user_id FROM sessions WHERE session_key = ?1",
                params![session_key],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_metadata_survives_restart_and_stays_in_its_session() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
        let mut message = ChatMessage::user("Summarize");
        message.files = serde_json::from_value(serde_json::json!([{
            "id": "file_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "name": "中文 file.csv",
            "mime_type": "text/csv", "size_bytes": 12, "format": "csv", "status": "ready",
            "unit": "record", "total": 2,
            "excerpt": {"attachment_id": "file_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "unit": "record",
                "start": 0, "next": 1, "total": 2, "eof": false, "content": "[record 1]\na,b\n"}
        }]))
        .unwrap();
        clawseed_api::file_attachment::validate_files(&message.files).unwrap();
        backend.append("file_a", &message).unwrap();
        backend
            .append("file_b", &ChatMessage::user("No attachment"))
            .unwrap();
        drop(backend);
        let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
        assert_eq!(backend.load("file_a")[0].files, message.files);
        assert_eq!(
            backend.load_with_presentations("file_a")[0].files,
            message.files
        );
        assert!(backend.load("file_b")[0].files.is_empty());
        assert_eq!(backend.load("file_a")[0].content, "Summarize");
        let prompt = backend.load("file_a")[0].with_file_context();
        assert_eq!(prompt.role, "user");
        assert!(prompt.content.contains("attachment_read"));
        assert!(prompt.content.contains("中文 file.csv"));
        let mut system = ChatMessage::system("system");
        system.files = message.files;
        assert_eq!(system.with_file_context().content, "system");
    }
    use clawseed_api::tool::{ContentBlock, ToolPresentation};

    fn fresh_backend() -> SqliteSessionBackend {
        let tmp = tempfile::tempdir().unwrap();
        SqliteSessionBackend::new(tmp.path()).unwrap()
        // tmp keeps the temp dir alive for the test via the backend's open conn
    }

    #[test]
    fn image_history_survives_reopen_enrichment_and_regeneration() {
        let tmp = tempfile::tempdir().unwrap();
        let mut message = ChatMessage::user("");
        message
            .attachments
            .push(clawseed_api::provider::ImageAttachment {
                id: "att_test".into(),
                mime_type: "image/png".into(),
                size_bytes: 128,
                width: 20,
                height: 30,
                resolved_path: Some(tmp.path().join("private.png")),
            });
        {
            let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
            backend.append("gw_images", &message).unwrap();
            backend
                .update_last_user("gw_images", &ChatMessage::user("enriched"))
                .unwrap();
            backend
                .append("gw_images", &ChatMessage::assistant("answer"))
                .unwrap();
            assert_eq!(
                backend.remove_last_assistant_turn("gw_images").as_deref(),
                Some("enriched")
            );
        }
        let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
        let history = backend.load("gw_images");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "enriched");
        message.attachments[0].resolved_path = None;
        assert_eq!(history[0].attachments, message.attachments);
        assert_eq!(
            backend.load_with_presentations("gw_images")[0].attachments,
            message.attachments
        );
        let conn = backend.conn.lock();
        let json: String = conn
            .query_row("SELECT attachments_json FROM messages", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!json.contains("private.png"));
        assert!(!json.contains("base64"));
    }

    #[test]
    fn legacy_database_migrates_without_changing_text() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("gateway")).unwrap();
        let conn = Connection::open(tmp.path().join("gateway/sessions.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT, session_key TEXT NOT NULL,
            role TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL
        ); INSERT INTO messages (session_key, role, content, created_at)
        VALUES ('gw_old', 'user', 'old text', '2026-09-09');",
        )
        .unwrap();
        drop(conn);
        for _ in 0..2 {
            let backend = SqliteSessionBackend::new(tmp.path()).unwrap();
            let history = backend.load("gw_old");
            assert_eq!(history[0].content, "old text");
            assert!(history[0].attachments.is_empty());
        }
    }

    #[test]
    fn persona_binding_set_get_roundtrip() {
        let b = fresh_backend();
        let key = "gw_s1";
        b.set_session_persona(key, Some("nova")).unwrap();
        assert_eq!(b.get_session_persona(key).unwrap().as_deref(), Some("nova"));
    }

    #[test]
    fn metrics_survive_reopen_and_are_removed_with_regeneration() {
        let dir = tempfile::tempdir().unwrap();
        let key = "gw_metrics";
        let expected = clawseed_api::provider::ResponseMetrics {
            input_tokens: Some(100),
            output_tokens: Some(20),
            cached_input_tokens: Some(0),
            cache_hit_ratio: Some(0.0),
            output_tokens_per_second: Some(10.0),
            elapsed_ms: 2500,
        };
        {
            let backend = SqliteSessionBackend::new(dir.path()).unwrap();
            backend.append(key, &ChatMessage::user("question")).unwrap();
            backend
                .append(key, &ChatMessage::assistant("answer"))
                .unwrap();
            backend.set_last_assistant_metrics(key, &expected).unwrap();
            // Updating an existing metrics record is idempotent.
            backend.set_last_assistant_metrics(key, &expected).unwrap();
            assert!(
                !serde_json::to_string(&backend.load(key))
                    .unwrap()
                    .contains("elapsed_ms")
            );
        }
        let backend = SqliteSessionBackend::new(dir.path()).unwrap();
        let transcript = backend.load_with_presentations(key);
        assert!(transcript[0].metrics.is_none());
        assert_eq!(transcript[1].metrics, Some(expected));
        backend.remove_last_assistant_turn(key);
        let count: i64 = backend
            .conn
            .lock()
            .query_row("SELECT COUNT(*) FROM message_metrics", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn presentation_round_trip_is_kept_out_of_llm_history() {
        let backend = fresh_backend();
        let key = "gw_rich_content";
        backend
            .append(key, &ChatMessage::user("find mars resources"))
            .unwrap();
        backend
            .append(key, &ChatMessage::assistant("Here are the results."))
            .unwrap();
        let presentation = ToolPresentation::new(vec![ContentBlock::SearchResults {
            query: "mars".into(),
            items: vec![],
        }]);
        backend
            .set_last_assistant_presentation(key, &presentation)
            .unwrap();

        let llm_history = backend.load(key);
        assert_eq!(llm_history[1].content, "Here are the results.");

        let transcript = backend.load_with_presentations(key);
        assert_eq!(transcript.len(), 2);
        assert_eq!(transcript[1].presentation, Some(presentation));
    }

    #[test]
    fn persona_binding_unset_returns_none() {
        let b = fresh_backend();
        assert!(b.get_session_persona("gw_never").unwrap().is_none());
    }

    #[test]
    fn persona_binding_clear_by_none() {
        let b = fresh_backend();
        let key = "gw_s2";
        b.set_session_persona(key, Some("nova")).unwrap();
        assert_eq!(b.get_session_persona(key).unwrap().as_deref(), Some("nova"));
        // Clearing with None removes the binding.
        b.set_session_persona(key, None).unwrap();
        assert!(b.get_session_persona(key).unwrap().is_none());
    }

    #[test]
    fn persona_binding_overwrite_same_key() {
        let b = fresh_backend();
        let key = "gw_s3";
        b.set_session_persona(key, Some("nova")).unwrap();
        b.set_session_persona(key, Some("analyst")).unwrap();
        assert_eq!(
            b.get_session_persona(key).unwrap().as_deref(),
            Some("analyst")
        );
    }

    #[test]
    fn persona_binding_keys_isolated() {
        let b = fresh_backend();
        b.set_session_persona("gw_a", Some("nova")).unwrap();
        b.set_session_persona("gw_b", Some("analyst")).unwrap();
        assert_eq!(
            b.get_session_persona("gw_a").unwrap().as_deref(),
            Some("nova")
        );
        assert_eq!(
            b.get_session_persona("gw_b").unwrap().as_deref(),
            Some("analyst")
        );
    }

    #[test]
    fn session_user_binding_is_immutable() {
        let backend = fresh_backend();
        assert!(backend.bind_session_user("gw_a", "owner").unwrap());
        assert_eq!(
            backend.get_session_user("gw_a").unwrap().as_deref(),
            Some("owner")
        );
        assert!(!backend.bind_session_user("gw_a", "other").unwrap());
        assert_eq!(
            backend.get_session_user("gw_a").unwrap().as_deref(),
            Some("owner")
        );
    }
}
