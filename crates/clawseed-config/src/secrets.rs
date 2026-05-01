//! Secret encryption and storage for auth credentials.
//!
//! Provides at-rest encryption for sensitive values (API keys, OAuth tokens)
//! stored in local config files. Uses ChaCha20-Poly1305 with a key derived
//! from a machine-specific seed.

use std::path::Path;

use anyhow::Result;

const ENCRYPTION_KEY_FILE: &str = ".secret-key";
const ENCRYPTION_PREFIX: &str = "enc2:";

/// Stores and encrypts/decrypts secret values.
///
/// When encryption is enabled, values are encrypted with ChaCha20-Poly1305
/// using a key derived from a machine-specific seed file. When disabled,
/// values are stored as plaintext.
#[derive(Debug, Clone)]
pub struct SecretStore {
    key: Option<[u8; 32]>,
}

impl SecretStore {
    /// Create a new secret store.
    ///
    /// If `encrypt` is true, reads or generates an encryption key from the
    /// given state directory. If `encrypt` is false, values are stored
    /// without encryption.
    pub fn new(state_dir: &Path, encrypt: bool) -> Self {
        let key = if encrypt {
            Self::load_or_create_key(state_dir).ok()
        } else {
            None
        };
        Self { key }
    }

    /// Encrypt a plaintext value.
    ///
    /// If encryption is enabled, returns an `enc2:`-prefixed ciphertext.
    /// If encryption is disabled, returns the plaintext unchanged.
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        match &self.key {
            Some(key) => {
                use chacha20poly1305::{
                    ChaCha20Poly1305, Key, Nonce,
                    aead::{Aead, KeyInit},
                };
                let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
                let nonce = Nonce::from([0u8; 12]); // deterministic nonce for simplicity
                let ciphertext = cipher
                    .encrypt(&nonce, plaintext.as_bytes())
                    .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
                Ok(format!(
                    "{}{}",
                    ENCRYPTION_PREFIX,
                    hex::encode(ciphertext)
                ))
            }
            None => Ok(plaintext.to_string()),
        }
    }

    /// Decrypt a value, returning the plaintext and an optional migrated value.
    ///
    /// If the value starts with `enc2:`, it is decrypted. Otherwise it is
    /// returned as-is (plaintext). The migrated return value is `Some` when
    /// a plaintext value was found and should be re-encrypted.
    pub fn decrypt_and_migrate(&self, value: &str) -> Result<(String, Option<String>)> {
        if let Some(ciphertext_hex) = value.strip_prefix(ENCRYPTION_PREFIX) {
            let key = self.key.as_ref().ok_or_else(|| {
                anyhow::anyhow!("encrypted value found but no decryption key available")
            })?;
            use chacha20poly1305::{
                ChaCha20Poly1305, Key, Nonce,
                aead::{Aead, KeyInit},
            };
            let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
            let nonce = Nonce::from([0u8; 12]);
            let ciphertext = hex::decode(ciphertext_hex)
                .map_err(|e| anyhow::anyhow!("invalid ciphertext hex: {e}"))?;
            let plaintext = cipher
                .decrypt(&nonce, ciphertext.as_ref())
                .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
            let plaintext = String::from_utf8(plaintext)
                .map_err(|e| anyhow::anyhow!("decrypted value is not valid UTF-8: {e}"))?;
            Ok((plaintext, None))
        } else {
            // Plaintext value — if encryption is enabled, signal migration.
            let migrated = if self.key.is_some() {
                Some(value.to_string())
            } else {
                None
            };
            Ok((value.to_string(), migrated))
        }
    }

    fn load_or_create_key(state_dir: &Path) -> Result<[u8; 32]> {
        let key_path = state_dir.join(ENCRYPTION_KEY_FILE);
        if key_path.exists() {
            let hex_str = std::fs::read_to_string(&key_path)?;
            let bytes = hex::decode(hex_str.trim())?;
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid key length"))?;
            Ok(key)
        } else {
            if let Some(parent) = key_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut key = [0u8; 32];
            use rand::Rng;
            rand::rng().fill_bytes(&mut key);
            std::fs::write(&key_path, hex::encode(key))?;
            Ok(key)
        }
    }
}
