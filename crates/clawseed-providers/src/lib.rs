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
pub use options::{api_error, provider_runtime_options_from_config, resolve_provider_credential, sanitize_api_error, ProviderRuntimeOptions};
pub use aliases::MAX_API_ERROR_CHARS;

/// Create a resilient provider with fallback and retry.
///
/// Matches the provider name to the correct implementation, wraps it in
/// `ReliableProvider` with the given reliability config and runtime options.
pub fn create_resilient_provider_with_options(
    provider_name: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    reliability: &clawseed_config::schema::ReliabilityConfig,
    _options: &ProviderRuntimeOptions,
) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
    use crate::anthropic::AnthropicProvider;
    use crate::bedrock::BedrockProvider;
    use crate::compatible::{AuthStyle, OpenAiCompatibleProvider};
    use crate::gemini::GeminiProvider;
    use crate::reliable::ReliableProvider;

    let resolved_key = resolve_provider_credential(provider_name, api_key);
    let key_ref = resolved_key.as_deref();

    let provider: Box<dyn clawseed_api::provider::Provider> = match provider_name {
        // ── Native Anthropic ──────────────────────────────
        "anthropic" => Box::new(AnthropicProvider::with_base_url(key_ref, base_url)),

        // ── Native Gemini ─────────────────────────────────
        "gemini" | "google" => Box::new(GeminiProvider::new(key_ref)),

        // ── Native Bedrock ────────────────────────────────
        "bedrock" | "aws-bedrock" => {
            let bp = if let Some(key) = key_ref {
                BedrockProvider::with_bearer_token(key)
            } else {
                BedrockProvider::new()
            };
            Box::new(bp)
        }

        // ── OpenRouter ────────────────────────────────────
        "openrouter" => Box::new(OpenAiCompatibleProvider::new(
            "OpenRouter",
            base_url.unwrap_or("https://openrouter.ai/api/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),

        // ── OpenAI ────────────────────────────────────────
        "openai" => Box::new(OpenAiCompatibleProvider::new(
            "OpenAI",
            base_url.unwrap_or("https://api.openai.com/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),

        // ── Ollama (local, no auth) ───────────────────────
        "ollama" | "llamacpp" | "llama.cpp" => Box::new(
            OpenAiCompatibleProvider::new(
                "Ollama",
                base_url.unwrap_or("http://localhost:11434/v1"),
                key_ref,
                AuthStyle::Bearer,
            )
            .without_native_tools(),
        ),

        // ── DeepSeek ──────────────────────────────────────
        "deepseek" => Box::new(OpenAiCompatibleProvider::new(
            "DeepSeek",
            base_url.unwrap_or("https://api.deepseek.com/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),

        // ── Groq ──────────────────────────────────────────
        "groq" => Box::new(OpenAiCompatibleProvider::new(
            "Groq",
            base_url.unwrap_or("https://api.groq.com/openai/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),

        // ── Mistral ───────────────────────────────────────
        "mistral" => Box::new(OpenAiCompatibleProvider::new(
            "Mistral",
            base_url.unwrap_or("https://api.mistral.ai/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),

        // ── xAI / Grok ────────────────────────────────────
        "xai" | "grok" => Box::new(OpenAiCompatibleProvider::new(
            "xAI",
            base_url.unwrap_or("https://api.x.ai/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),

        // ── China-region providers (resolved via aliases) ──
        name if aliases::is_glm_alias(name) => {
            let url = base_url.unwrap_or_else(|| aliases::glm_base_url(name).unwrap_or("https://api.z.ai/api/paas/v4"));
            Box::new(OpenAiCompatibleProvider::new(
                "GLM",
                url,
                key_ref,
                AuthStyle::ZhipuJwt,
            ))
        }
        name if aliases::is_minimax_alias(name) => {
            let url = base_url.unwrap_or_else(|| aliases::minimax_base_url(name).unwrap_or("https://api.minimax.io/v1"));
            Box::new(OpenAiCompatibleProvider::new(
                "MiniMax",
                url,
                key_ref,
                AuthStyle::Bearer,
            ).with_merge_system_into_user())
        }
        name if aliases::is_moonshot_alias(name) => {
            let url = base_url.unwrap_or_else(|| aliases::moonshot_base_url(name).unwrap_or("https://api.moonshot.cn/v1"));
            Box::new(OpenAiCompatibleProvider::new(
                "Moonshot",
                url,
                key_ref,
                AuthStyle::Bearer,
            ))
        }
        name if aliases::is_qwen_alias(name) => {
            let url = base_url.unwrap_or_else(|| aliases::qwen_base_url(name).unwrap_or("https://dashscope.aliyuncs.com/compatible-mode/v1"));
            Box::new(OpenAiCompatibleProvider::new_with_vision(
                "Qwen",
                url,
                key_ref,
                AuthStyle::Bearer,
                true,
            ))
        }
        name if aliases::is_bailian_alias(name) => {
            let url = base_url.unwrap_or(aliases::BAILIAN_BASE_URL);
            Box::new(OpenAiCompatibleProvider::new_with_vision(
                "Bailian",
                url,
                key_ref,
                AuthStyle::Bearer,
                true,
            ))
        }
        name if aliases::is_zai_alias(name) => {
            let url = base_url.unwrap_or_else(|| aliases::zai_base_url(name).unwrap_or("https://api.z.ai/api/coding/paas/v4"));
            Box::new(OpenAiCompatibleProvider::new(
                "Z.AI",
                url,
                key_ref,
                AuthStyle::ZhipuJwt,
            ))
        }
        name if aliases::is_qianfan_alias(name) => {
            let qianfan_url = aliases::qianfan_base_url(base_url);
            Box::new(OpenAiCompatibleProvider::new(
                "Qianfan",
                &qianfan_url,
                key_ref,
                AuthStyle::Bearer,
            ))
        }
        name if aliases::is_doubao_alias(name) => {
            let url = base_url.unwrap_or("https://ark.cn-beijing.volces.com/api/v3");
            Box::new(OpenAiCompatibleProvider::new(
                "Doubao",
                url,
                key_ref,
                AuthStyle::Bearer,
            ))
        }

        // ── Other well-known OpenAI-compatible providers ───
        "venice" => Box::new(OpenAiCompatibleProvider::new(
            "Venice",
            base_url.unwrap_or("https://api.venice.ai/api/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "together" | "together-ai" => Box::new(OpenAiCompatibleProvider::new(
            "Together",
            base_url.unwrap_or("https://api.together.xyz/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "fireworks" | "fireworks-ai" => Box::new(OpenAiCompatibleProvider::new(
            "Fireworks",
            base_url.unwrap_or("https://api.fireworks.ai/inference/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "perplexity" => Box::new(OpenAiCompatibleProvider::new(
            "Perplexity",
            base_url.unwrap_or("https://api.perplexity.ai"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "cohere" => Box::new(OpenAiCompatibleProvider::new(
            "Cohere",
            base_url.unwrap_or("https://api.cohere.ai/v2"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "novita" => Box::new(OpenAiCompatibleProvider::new(
            "Novita",
            base_url.unwrap_or("https://api.novita.ai/v3/openai"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "nvidia" | "nvidia-nim" | "build.nvidia.com" => Box::new(OpenAiCompatibleProvider::new(
            "NVIDIA",
            base_url.unwrap_or("https://integrate.api.nvidia.com/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "copilot" | "github-copilot" => Box::new(OpenAiCompatibleProvider::new(
            "GitHub Copilot",
            base_url.unwrap_or("https://api.githubcopilot.com"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "vercel" | "vercel-ai" => Box::new(OpenAiCompatibleProvider::new(
            "Vercel AI Gateway",
            base_url.unwrap_or(aliases::VERCEL_AI_GATEWAY_BASE_URL),
            key_ref,
            AuthStyle::Bearer,
        )),
        "cloudflare" | "cloudflare-ai" => Box::new(OpenAiCompatibleProvider::new(
            "Cloudflare AI",
            base_url.unwrap_or("https://gateway.ai.cloudflare.com/v1"),
            key_ref,
            AuthStyle::Bearer,
        )),
        "sglang" | "vllm" | "synthetic" | "osaurus" | "deepmyst" | "deep-myst"
        | "ovhcloud" | "ovh" | "astrai" | "avian" | "aihubmix"
        | "siliconflow" | "silicon-flow" | "telnyx"
        | "openai-codex" | "openai_codex" | "codex"
        | "kimi-code" | "kimi_coding" | "kimi_for_coding" => {
            // These providers all use the OpenAI-compatible format.
            // base_url is required (or must come from config/env).
            let url = base_url.unwrap_or_else(|| {
                tracing::warn!(
                    provider = provider_name,
                    "Provider requires base_url but none provided; using provider name as hint"
                );
                provider_name
            });
            Box::new(OpenAiCompatibleProvider::new(
                provider_name,
                url,
                key_ref,
                AuthStyle::Bearer,
            ))
        }

        // ── Azure OpenAI ──────────────────────────────────
        "azure_openai" | "azure-openai" | "azure" => {
            let url = match base_url {
                Some(u) => u,
                None => anyhow::bail!(
                    "Azure OpenAI requires a base URL. Format: https://<resource>.openai.azure.com/openai/deployments/<deployment>"
                ),
            };
            Box::new(OpenAiCompatibleProvider::new(
                "Azure OpenAI",
                url,
                key_ref,
                AuthStyle::Bearer,
            ))
        }

        // ── Fallback: treat as OpenAI-compatible ──────────
        _ => {
            let url = match base_url {
                Some(u) => u.to_string(),
                None => {
                    // If the provider name looks like a URL, use it directly.
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
            Box::new(OpenAiCompatibleProvider::new(
                provider_name,
                &url,
                key_ref,
                AuthStyle::Bearer,
            ))
        }
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
