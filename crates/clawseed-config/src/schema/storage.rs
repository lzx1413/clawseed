//! Storage configuration.

use serde::{Deserialize, Serialize};

/// Storage provider sub-configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageProviderConfig {
    /// Database URL.
    #[serde(default)]
    pub db_url: Option<String>,
}

/// Storage provider wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageProviderEntry {
    /// Provider name (e.g. "sqlite").
    #[serde(default = "default_storage_provider")]
    pub name: String,
    /// Provider-specific configuration.
    #[serde(default)]
    pub config: StorageProviderConfig,
}

fn default_storage_provider() -> String { "sqlite".into() }

impl Default for StorageProviderEntry {
    fn default() -> Self {
        Self {
            name: default_storage_provider(),
            config: StorageProviderConfig::default(),
        }
    }
}

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage provider entry.
    #[serde(default)]
    pub provider: StorageProviderEntry,

    /// Storage backend (e.g. "sqlite", "memory").
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Database URL (for SQLite: file path).
    #[serde(default)]
    pub db_url: Option<String>,

    /// Schema name.
    #[serde(default = "default_schema")]
    pub schema: String,

    /// Table name.
    #[serde(default = "default_table")]
    pub table: String,
}

fn default_backend() -> String { "sqlite".into() }
fn default_schema() -> String { "clawseed".into() }
fn default_table() -> String { "memories".into() }

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            provider: StorageProviderEntry::default(),
            backend: default_backend(),
            db_url: None,
            schema: default_schema(),
            table: default_table(),
        }
    }
}
