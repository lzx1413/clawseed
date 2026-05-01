use std::path::PathBuf;

use crate::aliases::{
    is_bailian_alias, is_doubao_alias, is_glm_alias, is_minimax_alias, is_moonshot_alias,
    is_qianfan_alias, is_qwen_alias, is_zai_alias, is_minimax_oauth_placeholder,
    MINIMAX_OAUTH_TOKEN_ENV, MINIMAX_API_KEY_ENV,
    resolve_minimax_oauth_refresh_token, resolve_minimax_static_credential,
    MAX_API_ERROR_CHARS,
};

#[derive(Debug, Clone)]
pub struct ProviderRuntimeOptions {
    pub auth_profile_override: Option<String>,
    pub provider_api_url: Option<String>,
    pub clawseed_dir: Option<PathBuf>,
    pub secrets_encrypt: bool,
    pub reasoning_enabled: Option<bool>,
    pub reasoning_effort: Option<String>,
    /// HTTP request timeout in seconds for LLM provider API calls.
    /// `None` uses the provider's built-in default (120s for compatible providers).
    pub provider_timeout_secs: Option<u64>,
    /// Extra HTTP headers to include in provider API requests.
    /// These are merged from the config file and `CLAWSEED_EXTRA_HEADERS` env var.
    pub extra_headers: std::collections::HashMap<String, String>,
    /// Custom API path suffix for OpenAI-compatible providers
    /// (e.g. "/v2/generate" instead of the default "/chat/completions").
    pub api_path: Option<String>,
    /// Maximum output tokens for LLM provider API requests.
    /// `None` uses the provider's built-in default.
    pub provider_max_tokens: Option<u32>,
    /// When true, system messages are merged into the first user message before
    /// sending. Propagated from `ModelProviderConfig::merge_system_into_user`.
    pub merge_system_into_user: bool,
    /// Extra JSON parameters merged into API request bodies at the top level.
    /// Propagated from `ModelProviderConfig::provider_extra`.
    pub provider_extra: Option<serde_json::Value>,
}

impl Default for ProviderRuntimeOptions {
    fn default() -> Self {
        Self {
            auth_profile_override: None,
            provider_api_url: None,
            clawseed_dir: None,
            secrets_encrypt: true,
            reasoning_enabled: None,
            reasoning_effort: None,
            provider_timeout_secs: None,
            extra_headers: std::collections::HashMap::new(),
            api_path: None,
            provider_max_tokens: None,
            merge_system_into_user: false,
            provider_extra: None,
        }
    }
}

pub fn provider_runtime_options_from_config(
    config: &clawseed_config::schema::Config,
) -> ProviderRuntimeOptions {
    let fallback = config.providers.fallback_provider();
    // Resolve merge_system_into_user from the active model provider profile by
    // matching api_url — apply_named_model_provider_profile() has already run
    // and rewritten providers.fallback, but providers.models retains all profiles.
    let merge_system_into_user = fallback
        .and_then(|e| e.base_url.as_deref())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .and_then(|active_url| {
            config.providers.models.values().find(|p| {
                p.base_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .map(|u| u.trim_end_matches('/'))
                    == Some(active_url.trim_end_matches('/'))
            })
        })
        .map(|p| p.merge_system_into_user)
        .unwrap_or(false);

    ProviderRuntimeOptions {
        auth_profile_override: None,
        provider_api_url: fallback.and_then(|e| e.base_url.clone()),
        clawseed_dir: config.config_path.parent().map(PathBuf::from),
        secrets_encrypt: config.secrets.encrypt,
        reasoning_enabled: config.runtime.reasoning_enabled,
        reasoning_effort: config.runtime.reasoning_effort.clone(),
        provider_timeout_secs: Some(fallback.and_then(|e| e.timeout_secs).unwrap_or(120)),
        extra_headers: fallback
            .map(|e| e.extra_headers.clone())
            .unwrap_or_default(),
        api_path: fallback.and_then(|e| e.api_path.clone()),
        provider_max_tokens: fallback.and_then(|e| e.max_tokens),
        merge_system_into_user,
        provider_extra: fallback.and_then(|e| e.provider_extra.clone()),
    }
}

fn is_secret_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')
}

fn token_end(input: &str, from: usize) -> usize {
    let mut end = from;
    for (i, c) in input[from..].char_indices() {
        if is_secret_char(c) {
            end = from + i + c.len_utf8();
        } else {
            break;
        }
    }
    end
}

/// Scrub known secret-like token prefixes from provider error strings.
///
/// Redacts tokens with prefixes like `sk-`, `xoxb-`, `xoxp-`, `ghp_`, `gho_`,
/// `ghu_`, and `github_pat_`.
pub fn scrub_secret_patterns(input: &str) -> String {
    const PREFIXES: [&str; 7] = [
        "sk-",
        "xoxb-",
        "xoxp-",
        "ghp_",
        "gho_",
        "ghu_",
        "github_pat_",
    ];

    let mut scrubbed = input.to_string();

    for prefix in PREFIXES {
        let mut search_from = 0;
        while let Some(rel) = scrubbed[search_from..].find(prefix) {
            let start = search_from + rel;
            let content_start = start + prefix.len();
            let end = token_end(&scrubbed, content_start);

            // Bare prefixes like "sk-" should not stop future scans.
            if end == content_start {
                search_from = content_start;
                continue;
            }

            scrubbed.replace_range(start..end, "[REDACTED]");
            search_from = start + "[REDACTED]".len();
        }
    }

    scrubbed
}

/// Sanitize API error text by scrubbing secrets and truncating length.
pub fn sanitize_api_error(input: &str) -> String {
    let scrubbed = scrub_secret_patterns(input);

    if scrubbed.chars().count() <= MAX_API_ERROR_CHARS {
        return scrubbed;
    }

    let mut end = MAX_API_ERROR_CHARS;
    while end > 0 && !scrubbed.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...", &scrubbed[..end])
}

/// Build a sanitized provider error from a failed HTTP response.
pub async fn api_error(provider: &str, response: reqwest::Response) -> anyhow::Error {
    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<failed to read provider error body>".to_string());
    let sanitized = sanitize_api_error(&body);
    anyhow::anyhow!("{provider} API error ({status}): {sanitized}")
}

/// Resolve API key for a provider from config and environment variables.
///
/// Resolution order:
/// 1. Explicitly provided `api_key` parameter (trimmed, filtered if empty)
/// 2. Provider-specific environment variable (e.g., `ANTHROPIC_OAUTH_TOKEN`, `OPENROUTER_API_KEY`)
/// 3. Generic fallback variables (`CLAWSEED_API_KEY`, `API_KEY`)
///
/// For Anthropic, the provider-specific env var is `ANTHROPIC_OAUTH_TOKEN` (for setup-tokens)
/// followed by `ANTHROPIC_API_KEY` (for regular API keys).
///
/// For MiniMax, OAuth mode supports `api_key = "minimax-oauth"`, resolving credentials from
/// `MINIMAX_OAUTH_TOKEN` first, then `MINIMAX_API_KEY`, and finally
/// `MINIMAX_OAUTH_REFRESH_TOKEN` (automatic access-token refresh).
pub fn resolve_provider_credential(name: &str, credential_override: Option<&str>) -> Option<String> {
    let mut minimax_oauth_placeholder_requested = false;

    if let Some(raw_override) = credential_override {
        let trimmed_override = raw_override.trim();
        if !trimmed_override.is_empty() {
            if is_minimax_alias(name) && is_minimax_oauth_placeholder(trimmed_override) {
                minimax_oauth_placeholder_requested = true;
                if let Some(credential) = resolve_minimax_static_credential() {
                    return Some(credential);
                }
                if let Some(credential) = resolve_minimax_oauth_refresh_token(name) {
                    return Some(credential);
                }
            } else if name == "anthropic" || name == "openai" || name == "groq" {
                // For well-known providers, prefer provider-specific env vars over the
                // global api_key override, since the global key may belong to a different
                // provider (e.g. a custom: gateway). This enables multi-provider setups
                // where the primary uses a custom gateway and fallbacks use named providers.
                let env_candidates: &[&str] = match name {
                    "anthropic" => &["ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"],
                    "openai" => &["OPENAI_API_KEY"],
                    "groq" => &["GROQ_API_KEY"],
                    _ => &[],
                };
                for env_var in env_candidates {
                    if let Ok(val) = std::env::var(env_var) {
                        let trimmed = val.trim().to_string();
                        if !trimmed.is_empty() {
                            return Some(trimmed);
                        }
                    }
                }
                return Some(trimmed_override.to_owned());
            } else {
                return Some(trimmed_override.to_owned());
            }
        }
    }

    let provider_env_candidates: Vec<&str> = match name {
        "anthropic" => vec!["ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"],
        "openrouter" => vec!["OPENROUTER_API_KEY"],
        "openai" => vec!["OPENAI_API_KEY"],
        "ollama" => vec!["OLLAMA_API_KEY"],
        "venice" => vec!["VENICE_API_KEY"],
        "groq" => vec!["GROQ_API_KEY"],
        "mistral" => vec!["MISTRAL_API_KEY"],
        "deepseek" => vec!["DEEPSEEK_API_KEY"],
        "xai" | "grok" => vec!["XAI_API_KEY"],
        "together" | "together-ai" => vec!["TOGETHER_API_KEY"],
        "fireworks" | "fireworks-ai" => vec!["FIREWORKS_API_KEY"],
        "novita" => vec!["NOVITA_API_KEY"],
        "perplexity" => vec!["PERPLEXITY_API_KEY"],
        "copilot" | "github-copilot" => vec!["GITHUB_TOKEN"],
        "cohere" => vec!["COHERE_API_KEY"],
        name if is_moonshot_alias(name) => vec!["MOONSHOT_API_KEY"],
        "kimi-code" | "kimi_coding" | "kimi_for_coding" => {
            vec!["KIMI_CODE_API_KEY", "MOONSHOT_API_KEY"]
        }
        name if is_glm_alias(name) => vec!["GLM_API_KEY"],
        name if is_minimax_alias(name) => vec![MINIMAX_OAUTH_TOKEN_ENV, MINIMAX_API_KEY_ENV],
        // Bedrock supports Bearer token auth via BEDROCK_API_KEY env var, in addition
        // to AWS AKSK (SigV4). If BEDROCK_API_KEY is set, return it; otherwise return
        // None and let BedrockProvider handle SigV4 credential resolution internally.
        "bedrock" | "aws-bedrock" => {
            if let Ok(val) = std::env::var("BEDROCK_API_KEY") {
                let trimmed = val.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
            return None;
        }
        name if is_qianfan_alias(name) => vec!["QIANFAN_API_KEY"],
        name if is_doubao_alias(name) => {
            vec!["ARK_API_KEY", "VOLCENGINE_API_KEY", "DOUBAO_API_KEY"]
        }
        name if is_qwen_alias(name) => vec!["DASHSCOPE_API_KEY"],
        name if is_bailian_alias(name) => vec!["BAILIAN_API_KEY", "DASHSCOPE_API_KEY"],
        name if is_zai_alias(name) => vec!["ZAI_API_KEY"],
        "nvidia" | "nvidia-nim" | "build.nvidia.com" => vec!["NVIDIA_API_KEY"],
        "synthetic" => vec!["SYNTHETIC_API_KEY"],
        "opencode" | "opencode-zen" => vec!["OPENCODE_API_KEY"],
        "opencode-go" => vec!["OPENCODE_GO_API_KEY"],
        "vercel" | "vercel-ai" => vec!["VERCEL_API_KEY"],
        "cloudflare" | "cloudflare-ai" => vec!["CLOUDFLARE_API_KEY"],
        "ovhcloud" | "ovh" => vec!["OVH_AI_ENDPOINTS_ACCESS_TOKEN"],
        "astrai" => vec!["ASTRAI_API_KEY"],
        "avian" => vec!["AVIAN_API_KEY"],
        "deepmyst" | "deep-myst" => vec!["DEEPMYST_API_KEY"],
        "llamacpp" | "llama.cpp" => vec!["LLAMACPP_API_KEY"],
        "sglang" => vec!["SGLANG_API_KEY"],
        "vllm" => vec!["VLLM_API_KEY"],
        "aihubmix" => vec!["AIHUBMIX_API_KEY"],
        "siliconflow" | "silicon-flow" => vec!["SILICONFLOW_API_KEY"],
        "osaurus" => vec!["OSAURUS_API_KEY"],
        "telnyx" => vec!["TELNYX_API_KEY"],
        "azure_openai" | "azure-openai" | "azure" => vec!["AZURE_OPENAI_API_KEY"],
        _ => vec![],
    };

    for env_var in provider_env_candidates {
        if let Ok(value) = std::env::var(env_var) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    if is_minimax_alias(name)
        && let Some(credential) = resolve_minimax_oauth_refresh_token(name)
    {
        return Some(credential);
    }

    if minimax_oauth_placeholder_requested && is_minimax_alias(name) {
        return None;
    }

    for env_var in ["CLAWSEED_API_KEY", "API_KEY"] {
        if let Ok(value) = std::env::var(env_var) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Check whether an API key's prefix matches the selected provider.
///
/// Returns `Some("likely_provider")` when the key clearly belongs to a
/// *different* provider (cross-provider mismatch).  Returns `None` when
/// everything looks fine or the format is unrecognised.
pub fn check_api_key_prefix(provider_name: &str, key: &str) -> Option<&'static str> {
    // Identify which provider the key likely belongs to (longest prefix first).
    let likely_provider = if key.starts_with("sk-ant-") {
        Some("anthropic")
    } else if key.starts_with("sk-or-") {
        Some("openrouter")
    } else if key.starts_with("sk-") {
        Some("openai")
    } else if key.starts_with("gsk_") {
        Some("groq")
    } else if key.starts_with("pplx-") {
        Some("perplexity")
    } else if key.starts_with("xai-") {
        Some("xai")
    } else if key.starts_with("nvapi-") {
        Some("nvidia")
    } else if key.starts_with("KEY-") {
        Some("telnyx")
    } else {
        None
    };

    let expected = likely_provider?;

    // Only flag mismatch for providers where we know the key format.
    let matches = match provider_name {
        "anthropic" => expected == "anthropic",
        "openrouter" => expected == "openrouter",
        "openai" => expected == "openai",
        "groq" => expected == "groq",
        "perplexity" => expected == "perplexity",
        "xai" | "grok" => expected == "xai",
        "nvidia" | "nvidia-nim" | "build.nvidia.com" => expected == "nvidia",
        "telnyx" => expected == "telnyx",
        _ => return None, // Unknown format provider — skip
    };

    if matches { None } else { Some(expected) }
}

pub fn parse_custom_provider_url(
    raw_url: &str,
    provider_label: &str,
    format_hint: &str,
) -> anyhow::Result<String> {
    let base_url = raw_url.trim();

    if base_url.is_empty() {
        anyhow::bail!("{provider_label} requires a URL. Format: {format_hint}");
    }

    let parsed = reqwest::Url::parse(base_url).map_err(|_| {
        anyhow::anyhow!("{provider_label} requires a valid URL. Format: {format_hint}")
    })?;

    match parsed.scheme() {
        "http" | "https" => Ok(base_url.to_string()),
        _ => anyhow::bail!(
            "{provider_label} requires an http:// or https:// URL. Format: {format_hint}"
        ),
    }
}
