//! LLM provider implementations for ClawSeed.
//!
//! Supports multiple API protocols:
//! - OpenAI-compatible (generic)
//! - Anthropic (native protocol)
//! - Google Gemini
//! - AWS Bedrock

pub mod aliases;
pub mod anthropic;
pub mod auth;
pub mod bedrock;
pub mod compatible;
pub mod gemini;
pub mod models_dev;
pub mod multimodal;
pub mod options;
pub mod registry;
pub mod reliable;
pub mod traits;

// Re-export utility functions so that submodules can reference them as
// `crate::sanitize_api_error`, `super::api_error`, etc.
pub use options::{api_error, provider_runtime_options_from_config, sanitize_api_error, ProviderRuntimeOptions};
pub use aliases::MAX_API_ERROR_CHARS;

/// Create a resilient provider with fallback and retry (stub).
pub fn create_resilient_provider_with_options(
    provider_name: &str,
    _api_key: Option<&str>,
    _base_url: Option<&str>,
    _reliability: &clawseed_config::schema::ReliabilityConfig,
    _options: &ProviderRuntimeOptions,
) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
    anyhow::bail!(
        "create_resilient_provider_with_options stub: provider '{}' not available in minimal crate",
        provider_name
    )
}
