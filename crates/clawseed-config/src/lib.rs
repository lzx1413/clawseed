//! Configuration loading and management for ClawSeed.
//!
//! Supports TOML config files with environment variable overrides (CLAWSEED_* prefix).
//!
//! # Environment Variable Overrides
//!
//! | Variable | Config Path |
//! |----------|-------------|
//! | `CLAWSEED_PROVIDER` | `providers.default` |
//! | `CLAWSEED_MODEL` | `providers.default_model` |
//! | `CLAWSEED_API_KEY` | `providers.default_api_key` |
//! | `CLAWSEED_PROVIDER_URL` | `providers.default_base_url` |
//! | `CLAWSEED_GATEWAY_HOST` | `gateway.host` |
//! | `CLAWSEED_GATEWAY_PORT` | `gateway.port` |
//! | `CLAWSEED_WORKSPACE` | `workspace_dir` |

pub mod schema;
pub mod secrets;

use std::path::{Path, PathBuf};

use anyhow::Result;
use schema::Config;

/// Find the workspace directory: `CLAWSEED_WORKSPACE` env > config file > current dir.
pub fn resolve_workspace_dir(config: &Config) -> PathBuf {
    if let Ok(ws) = std::env::var("CLAWSEED_WORKSPACE") {
        PathBuf::from(ws)
    } else if !config.workspace_dir.as_os_str().is_empty() {
        config.workspace_dir.clone()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Find the config file path.
pub fn find_config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CLAWSEED_CONFIG_DIR") {
        let p = PathBuf::from(p);
        let candidate = p.join("clawseed.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Some(dirs) = directories::ProjectDirs::from("", "", "clawseed") {
        let candidate = dirs.config_dir().join("clawseed.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let local = PathBuf::from("clawseed.toml");
    if local.exists() {
        return Some(local);
    }
    None
}

/// Load configuration from the default path with environment variable overrides.
pub fn load_config() -> Result<Config> {
    if let Some(path) = find_config_path() {
        Config::from_file(&path)
    } else {
        Ok(Config::with_env_overrides(Config::default()))
    }
}

/// Load configuration from a specific file.
pub fn load_config_from(path: &Path) -> Result<Config> {
    Config::from_file(path)
}
