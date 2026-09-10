//! Session backend trait for persisting gateway chat sessions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clawseed_api::provider::{ChatMessage, ImageAttachment};
use clawseed_api::tool::ToolPresentation;

/// A persisted transcript entry. Presentation data is intentionally separate
/// from `ChatMessage`: it is rendered by clients but never sent back to an LLM.
#[derive(Debug, Clone)]
pub struct PersistedMessage {
    pub role: String,
    pub content: String,
    pub attachments: Vec<ImageAttachment>,
    pub presentation: Option<ToolPresentation>,
    pub metrics: Option<clawseed_api::provider::ResponseMetrics>,
}

/// Metadata for a persisted session.
#[derive(Debug, Clone)]
pub struct SessionMetadata {
    /// Session key (e.g. "gw_<uuid>").
    pub key: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last active.
    pub last_activity: DateTime<Utc>,
    /// Number of messages in the session.
    pub message_count: usize,
    /// Optional human-readable name.
    pub name: Option<String>,
    /// Persona bound to this session, if any.
    pub persona: Option<String>,
    /// Authenticated user that owns this session.
    pub user_id: Option<String>,
}

/// Session state information.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Current state: "idle", "running", "error".
    pub state: String,
    /// ID of the current turn (if running).
    pub turn_id: Option<String>,
    /// When the current turn started (if running).
    pub turn_started_at: Option<DateTime<Utc>>,
}

/// Trait for session persistence backends.
#[async_trait]
pub trait SessionBackend: Send + Sync + 'static {
    fn upload_image(
        &self,
        _session_key: &str,
        _user_id: &str,
        _bytes: &[u8],
    ) -> anyhow::Result<ImageAttachment> {
        anyhow::bail!("Image attachments are not supported by this session backend")
    }

    fn resolve_images(
        &self,
        _session_key: &str,
        _user_id: &str,
        ids: &[String],
    ) -> anyhow::Result<Vec<ImageAttachment>> {
        anyhow::ensure!(
            ids.is_empty(),
            "Image attachments are not supported by this session backend"
        );
        Ok(Vec::new())
    }

    fn cleanup_images(&self) -> anyhow::Result<usize> {
        Ok(0)
    }

    /// Load all messages for a session.
    fn load(&self, session_key: &str) -> Vec<ChatMessage>;

    /// Load transcript entries for client history, including optional UI-only
    /// rich-content data. Backends that do not support it remain compatible.
    fn load_with_presentations(&self, session_key: &str) -> Vec<PersistedMessage> {
        self.load(session_key)
            .into_iter()
            .map(|message| PersistedMessage {
                role: message.role,
                content: message.content,
                attachments: message.attachments,
                presentation: None,
                metrics: None,
            })
            .collect()
    }

    /// Append a message to a session.
    fn append(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()>;

    /// Update the last assistant message in a session (for streaming partial content).
    fn update_last(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()>;

    /// Update the last user message in a session (to persist enriched content
    /// — timestamp prefix + memory context — so session resume preserves prompt
    /// cache fidelity).
    fn update_last_user(&self, session_key: &str, message: &ChatMessage) -> anyhow::Result<()>;

    /// Store client-facing rich content for the last assistant message in a
    /// completed turn. The plain assistant content remains the LLM history.
    fn set_last_assistant_presentation(
        &self,
        _session_key: &str,
        _presentation: &ToolPresentation,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// Store UI-only response metrics without adding them to model context.
    fn set_last_assistant_metrics(
        &self,
        _session_key: &str,
        _metrics: &clawseed_api::provider::ResponseMetrics,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    /// List all session keys.
    fn list_sessions(&self) -> Vec<String>;

    /// List sessions with metadata.
    fn list_sessions_with_metadata(&self) -> Vec<SessionMetadata>;

    /// Delete a session. Returns true if the session existed.
    fn delete_session(&self, session_key: &str) -> anyhow::Result<bool>;

    /// Set a human-readable name for a session.
    fn set_session_name(&self, session_key: &str, name: &str) -> anyhow::Result<()>;

    /// Get the human-readable name for a session.
    fn get_session_name(&self, session_key: &str) -> anyhow::Result<Option<String>>;

    /// Set the session state.
    fn set_session_state(
        &self,
        session_key: &str,
        state: &str,
        turn_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Get the session state.
    fn get_session_state(&self, session_key: &str) -> anyhow::Result<Option<SessionState>>;

    /// List sessions currently in "running" state.
    fn list_running_sessions(&self) -> Vec<SessionMetadata>;

    /// Clean up sessions older than the given TTL in hours.
    fn cleanup_stale(&self, ttl_hours: u32) -> anyhow::Result<usize>;

    /// Remove all messages after the last user message (inclusive).
    /// Returns the content of that last user message, or None if no user message exists.
    fn remove_last_assistant_turn(&self, session_key: &str) -> Option<String>;

    /// Bind a persona to a session (persona↔session binding persistence).
    ///
    /// `persona = None` clears the binding. The binding is write-once from the
    /// client's perspective: once set, `get_session_persona` is authoritative
    /// on resume and the client cannot change it by passing a different
    /// `?persona=` query param (see `ws::handle_socket`).
    fn set_session_persona(&self, session_key: &str, persona: Option<&str>) -> anyhow::Result<()>;

    /// Read the persona bound to a session, if any.
    fn get_session_persona(&self, session_key: &str) -> anyhow::Result<Option<String>>;

    /// Bind a session to a user. Returns false if another user already owns it.
    fn bind_session_user(&self, _session_key: &str, _user_id: &str) -> anyhow::Result<bool> {
        Ok(true)
    }

    /// Read the authenticated user bound to a session.
    fn get_session_user(&self, _session_key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}
