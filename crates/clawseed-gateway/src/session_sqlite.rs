//! SQLite-backed session persistence for the gateway.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clawseed_api::provider::ChatMessage;
use parking_lot::Mutex;
use rusqlite::{Connection, params};

use super::session_backend::{SessionBackend, SessionMetadata, SessionState};

pub struct SqliteSessionBackend {
    conn: Arc<Mutex<Connection>>,
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
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_key);",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
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
                    (SELECT COUNT(*) FROM messages m WHERE m.session_key = s.session_key) as msg_count
             FROM sessions s
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
                row.get::<_, usize>(4)?,
            ))
        }) else {
            return Vec::new();
        };
        rows.filter_map(|r| {
            let (key, created_str, activity_str, name, msg_count) = r.ok()?;
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
            })
        })
        .collect()
    }
}

#[async_trait]
impl SessionBackend for SqliteSessionBackend {
    fn load(&self, session_key: &str) -> Vec<ChatMessage> {
        let conn = self.conn.lock();
        let mut stmt = match conn
            .prepare("SELECT role, content FROM messages WHERE session_key = ?1 ORDER BY id")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let Ok(rows) = stmt.query_map(params![session_key], |row| {
            Ok(ChatMessage {
                role: row.get(0)?,
                content: row.get(1)?,
                stable_prefix: None, // Not persisted; rebuilt by seed_history on resume
            })
        }) else {
            return Vec::new();
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    fn append(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()> {
        let conn = self.conn.lock();
        self.ensure_session(&conn, session_key)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO messages (session_key, role, content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![session_key, message.role, message.content, now],
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
}
