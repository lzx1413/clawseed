//! Secret store stub for encrypted secrets.

/// Secret store for encrypting/decrypting sensitive values.
pub struct SecretStore {
    _workspace_dir: std::path::PathBuf,
    _encrypt: bool,
}

impl SecretStore {
    pub fn new(workspace_dir: &std::path::Path, encrypt: bool) -> Self {
        Self {
            _workspace_dir: workspace_dir.to_path_buf(),
            _encrypt: encrypt,
        }
    }

    /// Check if a value looks like an encrypted string.
    pub fn is_encrypted(_value: &str) -> bool {
        false
    }
}

/// WebAuthn configuration.
pub struct WebAuthnConfig {
    pub enabled: bool,
    pub rp_id: String,
    pub rp_origin: String,
    pub rp_name: String,
}

/// WebAuthn manager stub.
pub struct WebAuthnManager;

impl WebAuthnManager {
    pub fn new(
        _config: WebAuthnConfig,
        _secret_store: std::sync::Arc<SecretStore>,
        _workspace_dir: &std::path::Path,
    ) -> Self {
        Self
    }
}

/// WebAuthn module.
pub mod webauthn {
    pub use super::{WebAuthnConfig, WebAuthnManager};
}
