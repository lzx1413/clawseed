//! models.dev catalog stub.
//!
//! The real implementation fetches model lists from <https://models.dev>.
//! This stub returns empty lists so that the provider code compiles
//! without the full models.dev integration.

/// Return the list of model IDs for a given provider from the models.dev catalog.
///
/// Currently returns an empty list. The full implementation will fetch
/// from the models.dev API.
pub async fn list_models_for(_provider: &str) -> anyhow::Result<Vec<String>> {
    Ok(Vec::new())
}
