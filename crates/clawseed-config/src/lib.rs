//! Configuration loading and management for ClawSeed.
//!
//! Supports TOML config files with environment variable overrides (CLAWSEED_* prefix).
//!
//! # Default Config Directory
//!
//! ClawSeed uses `~/.clawseed/` as its default config directory, with the
//! config file at `~/.clawseed/clawseed.toml` and workspace at
//! `~/.clawseed/workspace/`. On first run, the directory and a default
//! config file are created automatically.
//!
//! # Config Search Order
//!
//! 1. `CLAWSEED_CONFIG_DIR` environment variable → `<dir>/clawseed.toml`
//! 2. `~/.clawseed/clawseed.toml`
//! 3. `./clawseed.toml` (current working directory)
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

use anyhow::{Context, Result};
use schema::Config;

/// Resolve the user's home directory.
///
/// Tries `$HOME` env var first, then falls back to the `directories` crate.
fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().to_path_buf()))
}

/// Return the default config directory: `~/.clawseed/`.
pub fn default_config_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".clawseed"))
}

/// Return the default workspace directory: `~/.clawseed/workspace/`.
pub fn default_workspace_dir() -> Option<PathBuf> {
    default_config_dir().map(|d| d.join("workspace"))
}

/// Find the workspace directory: `CLAWSEED_WORKSPACE` env > config file > `~/.clawseed/workspace/`.
pub fn resolve_workspace_dir(config: &Config) -> PathBuf {
    if let Ok(ws) = std::env::var("CLAWSEED_WORKSPACE") {
        PathBuf::from(ws)
    } else if !config.workspace_dir.as_os_str().is_empty() {
        config.workspace_dir.clone()
    } else if let Some(ws) = default_workspace_dir() {
        ws
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Find the config file path.
///
/// Search order:
/// 1. `CLAWSEED_CONFIG_DIR` env → `<dir>/clawseed.toml`
/// 2. `~/.clawseed/clawseed.toml`
/// 3. `./clawseed.toml`
pub fn find_config_path() -> Option<PathBuf> {
    // 1. Explicit env override
    if let Ok(p) = std::env::var("CLAWSEED_CONFIG_DIR") {
        let candidate = PathBuf::from(p).join("clawseed.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // 2. ~/.clawseed/clawseed.toml
    if let Some(home) = home_dir() {
        let candidate = home.join(".clawseed").join("clawseed.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // 3. ./clawseed.toml
    let local = PathBuf::from("clawseed.toml");
    if local.exists() {
        return Some(local);
    }
    None
}

/// Load configuration, auto-creating directory and default config on first run.
///
/// If no config file exists, creates `~/.clawseed/` and writes a default
/// `clawseed.toml` with sensible provider and gateway values.
pub fn load_or_init_config() -> Result<Config> {
    if let Some(path) = find_config_path() {
        return Config::from_file(&path);
    }

    // No config file found — create default directory and config
    let config_dir =
        default_config_dir().context("Cannot determine home directory for ~/.clawseed/")?;
    std::fs::create_dir_all(&config_dir).with_context(|| {
        format!(
            "Failed to create config directory: {}",
            config_dir.display()
        )
    })?;

    let config_path = config_dir.join("clawseed.toml");
    let default_toml = Config::default_toml();
    std::fs::write(&config_path, &default_toml)
        .with_context(|| format!("Failed to write default config: {}", config_path.display()))?;

    tracing::info!("Created default config at {}", config_path.display());

    Config::from_file(&config_path)
}

/// Load configuration from the default path with environment variable overrides.
///
/// On first run (no config file exists), auto-creates `~/.clawseed/` with a
/// default configuration.
pub fn load_config() -> Result<Config> {
    load_or_init_config()
}

/// Load configuration from a specific file.
pub fn load_config_from(path: &Path) -> Result<Config> {
    Config::from_file(path)
}
