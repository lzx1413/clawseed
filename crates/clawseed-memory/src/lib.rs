//! Memory storage and retrieval for ClawSeed.
//!
//! SQLite-backed memory with vector search, decay, and importance scoring.

pub mod backend;
pub mod chunker;
pub mod conflict;
pub mod consolidation;
pub mod decay;
pub mod embeddings;
pub mod hygiene;
pub mod importance;
pub mod namespaced;
pub mod none;
pub mod retrieval;
pub mod snapshot;
pub mod sqlite;
pub mod traits;
pub mod vector;

use clawseed_api::memory_traits::Memory;
use clawseed_config::schema::MemoryConfig;
use std::sync::Arc;

/// Create a memory backend based on the configuration.
///
/// Returns `NoneMemory` when the backend is "none" or when SQLite
/// initialization fails (graceful degradation). Returns `SqliteMemory`
/// for the "sqlite" backend.
///
/// Also runs hygiene (if due) and auto-hydration (if brain.db is missing).
pub fn create_memory(
    config: &MemoryConfig,
    workspace_dir: &std::path::Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn Memory>> {
    // Best-effort hygiene pass (throttled by state file).
    if let Err(e) = hygiene::run_if_due(config, workspace_dir) {
        tracing::warn!("memory hygiene skipped: {e}");
    }

    // Snapshot after hygiene if enabled.
    if config.snapshot_enabled
        && let Err(e) = snapshot::export_snapshot(workspace_dir)
    {
        tracing::warn!("memory snapshot skipped: {e}");
    }

    // Auto-hydrate from snapshot if brain.db is missing.
    if config.auto_hydrate && snapshot::should_hydrate(workspace_dir) {
        tracing::info!("Cold boot detected — hydrating from MEMORY_SNAPSHOT.md");
        match snapshot::hydrate_from_snapshot(workspace_dir) {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Hydrated {count} core memories from snapshot");
                }
            }
            Err(e) => {
                tracing::warn!("memory hydration failed: {e}");
            }
        }
    }

    match config.backend.as_str() {
        "none" => Ok(Arc::new(none::NoneMemory::new())),
        _ => {
            // Default to SQLite; fall back to NoneMemory on error.
            match sqlite::SqliteMemory::new(workspace_dir) {
                Ok(m) => Ok(Arc::new(m)),
                Err(e) => {
                    tracing::warn!(
                        "Failed to create SQLite memory, falling back to NoneMemory: {e}"
                    );
                    Ok(Arc::new(none::NoneMemory::new()))
                }
            }
        }
    }
}

/// Create a memory backend with storage and routes.
///
/// Delegates to `create_memory()` for the actual backend. The additional
/// parameters (embedding routes, storage config) are reserved for future
/// vector-search and consolidated storage support.
pub fn create_memory_with_storage_and_routes(
    config: &MemoryConfig,
    _embedding_routes: &clawseed_config::schema::ProvidersConfig,
    _storage_config: Option<&clawseed_config::schema::StorageConfig>,
    workspace_dir: &std::path::Path,
    api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn Memory>> {
    create_memory(config, workspace_dir, api_key)
}

/// Check if content should be skipped for autosave.
///
/// Filters out noise from automated tasks and system-generated content
/// that would pollute memory with low-value entries.
pub fn should_skip_autosave_content(content: &str) -> bool {
    if content.trim().is_empty() {
        return true;
    }
    let trimmed = content.trim_start();
    let lower = trimmed.to_lowercase();
    trimmed.starts_with("[cron:")
        || trimmed.starts_with("[heartbeat")
        || trimmed.starts_with("[distilled_")
        || lower.starts_with("[memory context]")
}
