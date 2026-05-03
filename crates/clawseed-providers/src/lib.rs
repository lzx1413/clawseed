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
pub mod factory;
pub mod gemini;
pub mod models_dev;
pub mod multimodal;
pub mod options;
pub mod registry;
pub mod reliable;
pub mod traits;

// Re-export utility functions so that submodules can reference them as
// `crate::sanitize_api_error`, `super::api_error`, etc.
pub use aliases::MAX_API_ERROR_CHARS;
pub use options::{
    ProviderRuntimeOptions, api_error, provider_runtime_options_from_config,
    resolve_provider_credential, sanitize_api_error,
};

/// Create a resilient provider using a specific factory registry.
///
/// This is the core implementation. The public `create_resilient_provider_with_options`
/// delegates here with the default registry.
pub fn create_resilient_provider_with_registry(
    registry: &factory::ProviderFactoryRegistry,
    provider_name: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    reliability: &clawseed_config::schema::ReliabilityConfig,
    options: &ProviderRuntimeOptions,
) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
    use crate::compatible::{AuthStyle, OpenAiCompatibleProvider};
    use crate::reliable::ReliableProvider;

    let resolved_key = resolve_provider_credential(provider_name, api_key);
    let key_ref = resolved_key.as_deref();

    let provider: Box<dyn clawseed_api::provider::Provider> = if let Some(f) =
        registry.get(provider_name)
    {
        f.create(provider_name, key_ref, base_url, options)?
    } else {
        // Fallback: treat as OpenAI-compatible
        let url = match base_url {
            Some(u) => u.to_string(),
            None => {
                if provider_name.starts_with("http://") || provider_name.starts_with("https://") {
                    provider_name.to_string()
                } else {
                    anyhow::bail!(
                        "Unknown provider '{}'. Set base_url to use it as an OpenAI-compatible endpoint, \
                         or choose a known provider name.",
                        provider_name
                    );
                }
            }
        };
        let mut p = OpenAiCompatibleProvider::new(
            provider_name,
            &url,
            key_ref,
            AuthStyle::Bearer,
        );
        if let Some(extra) = options.provider_extra.clone() {
            p = p.with_provider_extra(extra);
        }
        Box::new(p)
    };

    // Wrap in ReliableProvider with retry/backoff and extra API keys.
    let reliable = ReliableProvider::new(
        vec![(provider_name.to_string(), provider)],
        reliability.max_retries,
        reliability.provider_backoff_ms,
    )
    .with_api_keys(reliability.api_keys.clone());

    Ok(Box::new(reliable))
}

/// Create a resilient provider with fallback and retry.
///
/// Uses the default provider factory registry to look up the provider by name (or alias),
/// then wraps it in `ReliableProvider` with the given reliability config.
pub fn create_resilient_provider_with_options(
    provider_name: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    reliability: &clawseed_config::schema::ReliabilityConfig,
    options: &ProviderRuntimeOptions,
) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
    static REGISTRY: std::sync::LazyLock<factory::ProviderFactoryRegistry> =
        std::sync::LazyLock::new(factory::default_provider_factory_registry);
    create_resilient_provider_with_registry(
        &REGISTRY,
        provider_name,
        api_key,
        base_url,
        reliability,
        options,
    )
}

/// Create a resilient provider with fallback and retry using default runtime options.
pub fn create_resilient_provider(
    provider_name: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    reliability: &clawseed_config::schema::ReliabilityConfig,
) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
    create_resilient_provider_with_options(
        provider_name,
        api_key,
        base_url,
        reliability,
        &ProviderRuntimeOptions::default(),
    )
}
