//! SQLite-backed session persistence for the gateway.

use async_trait::async_trait;
use clawseed_api::provider::ChatMessage;

use super::session_backend::{SessionBackend, SessionMetadata, SessionState};

/// SQLite-backed session persistence.
pub struct SqliteSessionBackend {
    _workspace_dir: std::path::PathBuf,
}

impl SqliteSessionBackend {
    pub fn new(workspace_dir: &std::path::Path) -> anyhow::Result<Self> {
        // TODO: implement SQLite session persistence
        Ok(Self {
            _workspace_dir: workspace_dir.to_path_buf(),
        })
    }
}

#[async_trait]
impl SessionBackend for SqliteSessionBackend {
    fn load(&self, _session_key: &str) -> Vec<ChatMessage> {
        Vec::new()
    }

    fn append(&self, _session_key: &str, _message: &ChatMessage) -> anyhow::Result<()> {
        Ok(())
    }

    fn update_last(&self, _session_key: &str, _message: &ChatMessage) -> anyhow::Result<()> {
        Ok(())
    }

    fn list_sessions(&self) -> Vec<String> {
        Vec::new()
    }

    fn list_sessions_with_metadata(&self) -> Vec<SessionMetadata> {
        Vec::new()
    }

    fn delete_session(&self, _session_key: &str) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn set_session_name(&self, _session_key: &str, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_session_name(&self, _session_key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    fn set_session_state(
        &self,
        _session_key: &str,
        _state: &str,
        _turn_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_session_state(&self, _session_key: &str) -> anyhow::Result<Option<SessionState>> {
        Ok(None)
    }

    fn list_running_sessions(&self) -> Vec<SessionMetadata> {
        Vec::new()
    }

    fn cleanup_stale(&self, _ttl_hours: u32) -> anyhow::Result<usize> {
        Ok(0)
    }
}
