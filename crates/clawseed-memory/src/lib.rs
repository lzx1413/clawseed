//! Memory storage and retrieval for ClawSeed.
//!
//! SQLite-backed memory with vector search, decay, and importance scoring.

pub mod backend;
pub mod chunker;
pub mod consolidation;
pub mod decay;
pub mod embeddings;
pub mod importance;
pub mod namespaced;
pub mod none;
pub mod retrieval;
pub mod sqlite;
pub mod traits;
pub mod vector;

use std::sync::Arc;
use clawseed_api::memory_traits::Memory;
use clawseed_config::schema::MemoryConfig;

/// Create a memory backend based on the configuration.
///
/// Returns `NoneMemory` when the backend is "none" or when SQLite
/// initialization fails (graceful degradation). Returns `SqliteMemory`
/// for the "sqlite" backend.
pub fn create_memory(
    config: &MemoryConfig,
    workspace_dir: &std::path::Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn Memory>> {
    match config.backend.as_str() {
        "none" => Ok(Arc::new(none::NoneMemory::new())),
        _ => {
            // Default to SQLite; fall back to NoneMemory on error.
            match sqlite::SqliteMemory::new(workspace_dir) {
                Ok(m) => Ok(Arc::new(m)),
                Err(e) => {
                    tracing::warn!("Failed to create SQLite memory, falling back to NoneMemory: {e}");
                    Ok(Arc::new(none::NoneMemory::new()))
                }
            }
        }
    }
}

/// Create a memory backend with storage and routes (stub).
pub fn create_memory_with_storage_and_routes(
    _config: &MemoryConfig,
    _embedding_routes: &clawseed_config::schema::ProvidersConfig,
    _storage_config: Option<&clawseed_config::schema::StorageConfig>,
    _workspace_dir: &std::path::Path,
    _api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn Memory>> {
    Ok(Arc::new(none::NoneMemory::new()))
}

/// Check if content should be skipped for autosave (stub).
pub fn should_skip_autosave_content(_content: &str) -> bool {
    false
}
