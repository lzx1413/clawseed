//! Provider factory registry for extensible provider creation.
//!
//! Replaces the monolithic match chain in `create_resilient_provider_with_options()`
//! with a registry of per-provider factories. Each factory implements
//! `ProviderFactory` and is registered by primary name and aliases.

use std::collections::HashMap;
use std::sync::Arc;

use crate::compatible::AuthStyle;

/// Trait for creating LLM providers.
///
/// Each factory knows how to create a specific provider (or family of providers).
/// The registry looks up the factory by name (or alias), then delegates construction.
pub trait ProviderFactory: Send + Sync {
    /// The primary provider name this factory handles (e.g. "anthropic", "openai").
    fn name(&self) -> &str;

    /// Alternative names that also route to this factory (e.g. "google" for Gemini).
    fn aliases(&self) -> &[&str] {
        &[]
    }

    /// Create a provider instance.
    ///
    /// `provider_name` is the original name the caller used (which may be an alias).
    /// Needed for China-region providers where the base URL depends on which alias was used.
    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>>;
}

/// Registry of provider factories, keyed by primary name and aliases.
pub struct ProviderFactoryRegistry {
    factories: HashMap<String, Arc<dyn ProviderFactory>>,
}

impl ProviderFactoryRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a factory under its primary name and all aliases.
    pub fn register(&mut self, factory: impl ProviderFactory + 'static) {
        let arc: Arc<dyn ProviderFactory> = Arc::new(factory);
        self.factories
            .insert(arc.name().to_string(), Arc::clone(&arc));
        for alias in arc.aliases() {
            self.factories.insert(alias.to_string(), Arc::clone(&arc));
        }
    }

    /// Look up a factory by name or alias.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ProviderFactory>> {
        self.factories.get(name).cloned()
    }
}

impl Default for ProviderFactoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Native providers ──────────────────────────────────────

struct AnthropicFactory;

impl ProviderFactory for AnthropicFactory {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        Ok(Box::new(
            crate::anthropic::AnthropicProvider::with_base_url(api_key, base_url),
        ))
    }
}

struct GeminiFactory;

impl ProviderFactory for GeminiFactory {
    fn name(&self) -> &str {
        "gemini"
    }

    fn aliases(&self) -> &[&str] {
        &["google"]
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        _base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        Ok(Box::new(crate::gemini::GeminiProvider::new(api_key)))
    }
}

struct BedrockFactory;

impl ProviderFactory for BedrockFactory {
    fn name(&self) -> &str {
        "bedrock"
    }

    fn aliases(&self) -> &[&str] {
        &["aws-bedrock"]
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        _base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let bp = if let Some(key) = api_key {
            crate::bedrock::BedrockProvider::with_bearer_token(key)
        } else {
            crate::bedrock::BedrockProvider::new()
        };
        Ok(Box::new(bp))
    }
}

// ─── Parameterized OpenAI-compatible factory ────────────────

/// Factory for providers that use the OpenAI-compatible chat completions API.
///
/// Most providers differ only in display name, default base URL, and auth style.
/// This parameterized struct handles all of them with a single implementation.
struct OpenAiCompatFactory {
    name: &'static str,
    display_name: &'static str,
    default_base_url: &'static str,
    auth_style: AuthStyle,
    no_native_tools: bool,
    with_vision: bool,
    merge_system_into_user: bool,
    extra_aliases: &'static [&'static str],
}

impl OpenAiCompatFactory {
    fn new(name: &'static str, display_name: &'static str, default_base_url: &'static str) -> Self {
        Self {
            name,
            display_name,
            default_base_url,
            auth_style: AuthStyle::Bearer,
            no_native_tools: false,
            with_vision: false,
            merge_system_into_user: false,
            extra_aliases: &[],
        }
    }

    fn without_native_tools(mut self) -> Self {
        self.no_native_tools = true;
        self
    }

    #[allow(dead_code)]
    fn with_vision(mut self) -> Self {
        self.with_vision = true;
        self
    }

    #[allow(dead_code)]
    fn merge_system_into_user(mut self) -> Self {
        self.merge_system_into_user = true;
        self
    }

    fn aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.extra_aliases = aliases;
        self
    }
}

impl ProviderFactory for OpenAiCompatFactory {
    fn name(&self) -> &str {
        self.name
    }

    fn aliases(&self) -> &[&str] {
        self.extra_aliases
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or(self.default_base_url);
        let mut provider = if self.with_vision {
            crate::compatible::OpenAiCompatibleProvider::new_with_vision(
                self.display_name,
                url,
                api_key,
                self.auth_style.clone(),
                true,
            )
        } else {
            crate::compatible::OpenAiCompatibleProvider::new(
                self.display_name,
                url,
                api_key,
                self.auth_style.clone(),
            )
        };

        if self.no_native_tools {
            provider = provider.without_native_tools();
        }
        if self.merge_system_into_user {
            provider = provider.with_merge_system_into_user();
        }

        Ok(Box::new(provider))
    }
}

// ─── Generic OpenAI-compatible factory (provider_name as fallback URL) ──

/// Factory for known providers that don't have a fixed default base URL.
///
/// Uses the provider name as the URL hint when base_url is not provided,
/// with a warning. This covers providers like sglang, vllm, siliconflow, etc.
struct GenericCompatFactory {
    names: &'static [&'static str],
}

impl ProviderFactory for GenericCompatFactory {
    fn name(&self) -> &str {
        self.names[0]
    }

    fn aliases(&self) -> &[&str] {
        &self.names[1..]
    }

    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or_else(|| {
            tracing::warn!(
                provider = provider_name,
                "Provider requires base_url but none provided; using provider name as hint"
            );
            provider_name
        });
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            provider_name,
            url,
            api_key,
            AuthStyle::Bearer,
        )))
    }
}

// ─── Azure OpenAI factory ──────────────────────────────────

struct AzureOpenAiFactory;

impl ProviderFactory for AzureOpenAiFactory {
    fn name(&self) -> &str {
        "azure_openai"
    }

    fn aliases(&self) -> &[&str] {
        &["azure-openai", "azure"]
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = match base_url {
            Some(u) => u,
            None => anyhow::bail!(
                "Azure OpenAI requires a base URL. Format: https://<resource>.openai.azure.com/openai/deployments/<deployment>"
            ),
        };
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            "Azure OpenAI",
            url,
            api_key,
            AuthStyle::Bearer,
        )))
    }
}

// ─── China-region provider factories ───────────────────────

struct GlmFactory;

impl ProviderFactory for GlmFactory {
    fn name(&self) -> &str {
        "glm"
    }

    fn aliases(&self) -> &[&str] {
        &[
            "zhipu",
            "glm-global",
            "zhipu-global",
            "glm-cn",
            "zhipu-cn",
            "bigmodel",
        ]
    }

    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or_else(|| {
            crate::aliases::glm_base_url(provider_name).unwrap_or("https://api.z.ai/api/paas/v4")
        });
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            "GLM",
            url,
            api_key,
            AuthStyle::ZhipuJwt,
        )))
    }
}

struct MinimaxFactory;

impl ProviderFactory for MinimaxFactory {
    fn name(&self) -> &str {
        "minimax"
    }

    fn aliases(&self) -> &[&str] {
        &[
            "minimax-intl",
            "minimax-io",
            "minimax-global",
            "minimax-oauth",
            "minimax-portal",
            "minimax-oauth-global",
            "minimax-portal-global",
            "minimax-cn",
            "minimaxi",
            "minimax-oauth-cn",
            "minimax-portal-cn",
        ]
    }

    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or_else(|| {
            crate::aliases::minimax_base_url(provider_name).unwrap_or("https://api.minimax.io/v1")
        });
        Ok(Box::new(
            crate::compatible::OpenAiCompatibleProvider::new(
                "MiniMax",
                url,
                api_key,
                AuthStyle::Bearer,
            )
            .with_merge_system_into_user(),
        ))
    }
}

struct MoonshotFactory;

impl ProviderFactory for MoonshotFactory {
    fn name(&self) -> &str {
        "moonshot"
    }

    fn aliases(&self) -> &[&str] {
        &[
            "kimi",
            "moonshot-cn",
            "kimi-cn",
            "moonshot-intl",
            "moonshot-global",
            "kimi-intl",
            "kimi-global",
        ]
    }

    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or_else(|| {
            crate::aliases::moonshot_base_url(provider_name).unwrap_or("https://api.moonshot.cn/v1")
        });
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            "Moonshot",
            url,
            api_key,
            AuthStyle::Bearer,
        )))
    }
}

struct QwenFactory;

impl ProviderFactory for QwenFactory {
    fn name(&self) -> &str {
        "qwen"
    }

    fn aliases(&self) -> &[&str] {
        &[
            "dashscope",
            "qwen-cn",
            "dashscope-cn",
            "qwen-intl",
            "dashscope-intl",
            "qwen-international",
            "dashscope-international",
            "qwen-us",
            "dashscope-us",
            "qwen-code",
            "qwen-oauth",
            "qwen_oauth",
        ]
    }

    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or_else(|| {
            crate::aliases::qwen_base_url(provider_name)
                .unwrap_or("https://dashscope.aliyuncs.com/compatible-mode/v1")
        });
        Ok(Box::new(
            crate::compatible::OpenAiCompatibleProvider::new_with_vision(
                "Qwen",
                url,
                api_key,
                AuthStyle::Bearer,
                true,
            ),
        ))
    }
}

struct BailianFactory;

impl ProviderFactory for BailianFactory {
    fn name(&self) -> &str {
        "bailian"
    }

    fn aliases(&self) -> &[&str] {
        &["aliyun-bailian", "aliyun"]
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or(crate::aliases::BAILIAN_BASE_URL);
        Ok(Box::new(
            crate::compatible::OpenAiCompatibleProvider::new_with_vision(
                "Bailian",
                url,
                api_key,
                AuthStyle::Bearer,
                true,
            ),
        ))
    }
}

struct ZaiFactory;

impl ProviderFactory for ZaiFactory {
    fn name(&self) -> &str {
        "zai"
    }

    fn aliases(&self) -> &[&str] {
        &["z.ai", "zai-global", "z.ai-global", "zai-cn", "z.ai-cn"]
    }

    fn create(
        &self,
        provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or_else(|| {
            crate::aliases::zai_base_url(provider_name)
                .unwrap_or("https://api.z.ai/api/coding/paas/v4")
        });
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            "Z.AI",
            url,
            api_key,
            AuthStyle::ZhipuJwt,
        )))
    }
}

struct QianfanFactory;

impl ProviderFactory for QianfanFactory {
    fn name(&self) -> &str {
        "qianfan"
    }

    fn aliases(&self) -> &[&str] {
        &["baidu"]
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let qianfan_url = crate::aliases::qianfan_base_url(base_url);
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            "Qianfan",
            &qianfan_url,
            api_key,
            AuthStyle::Bearer,
        )))
    }
}

struct DoubaoFactory;

impl ProviderFactory for DoubaoFactory {
    fn name(&self) -> &str {
        "doubao"
    }

    fn aliases(&self) -> &[&str] {
        &["volcengine", "ark", "doubao-cn"]
    }

    fn create(
        &self,
        _provider_name: &str,
        api_key: Option<&str>,
        base_url: Option<&str>,
        _options: &crate::options::ProviderRuntimeOptions,
    ) -> anyhow::Result<Box<dyn clawseed_api::provider::Provider>> {
        let url = base_url.unwrap_or("https://ark.cn-beijing.volces.com/api/v3");
        Ok(Box::new(crate::compatible::OpenAiCompatibleProvider::new(
            "Doubao",
            url,
            api_key,
            AuthStyle::Bearer,
        )))
    }
}

// ─── Default registry ──────────────────────────────────────

/// Build the default provider factory registry with all built-in providers.
pub fn default_provider_factory_registry() -> ProviderFactoryRegistry {
    let mut reg = ProviderFactoryRegistry::new();

    // ── Native providers ──
    reg.register(AnthropicFactory);
    reg.register(GeminiFactory);
    reg.register(BedrockFactory);

    // ── OpenAI-compatible providers with known URLs ──
    reg.register(OpenAiCompatFactory::new(
        "openrouter",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
    ));
    reg.register(OpenAiCompatFactory::new(
        "openai",
        "OpenAI",
        "https://api.openai.com/v1",
    ));
    reg.register(
        OpenAiCompatFactory::new("ollama", "Ollama", "http://localhost:11434/v1")
            .without_native_tools()
            .aliases(&["llamacpp", "llama.cpp"]),
    );
    reg.register(OpenAiCompatFactory::new(
        "deepseek",
        "DeepSeek",
        "https://api.deepseek.com/v1",
    ));
    reg.register(OpenAiCompatFactory::new(
        "groq",
        "Groq",
        "https://api.groq.com/openai/v1",
    ));
    reg.register(OpenAiCompatFactory::new(
        "mistral",
        "Mistral",
        "https://api.mistral.ai/v1",
    ));
    reg.register(OpenAiCompatFactory::new("xai", "xAI", "https://api.x.ai/v1").aliases(&["grok"]));
    reg.register(OpenAiCompatFactory::new(
        "venice",
        "Venice",
        "https://api.venice.ai/api/v1",
    ));
    reg.register(
        OpenAiCompatFactory::new("together", "Together", "https://api.together.xyz/v1")
            .aliases(&["together-ai"]),
    );
    reg.register(
        OpenAiCompatFactory::new(
            "fireworks",
            "Fireworks",
            "https://api.fireworks.ai/inference/v1",
        )
        .aliases(&["fireworks-ai"]),
    );
    reg.register(OpenAiCompatFactory::new(
        "perplexity",
        "Perplexity",
        "https://api.perplexity.ai",
    ));
    reg.register(OpenAiCompatFactory::new(
        "cohere",
        "Cohere",
        "https://api.cohere.ai/v2",
    ));
    reg.register(OpenAiCompatFactory::new(
        "novita",
        "Novita",
        "https://api.novita.ai/v3/openai",
    ));
    reg.register(
        OpenAiCompatFactory::new("nvidia", "NVIDIA", "https://integrate.api.nvidia.com/v1")
            .aliases(&["nvidia-nim", "build.nvidia.com"]),
    );
    reg.register(
        OpenAiCompatFactory::new("copilot", "GitHub Copilot", "https://api.githubcopilot.com")
            .aliases(&["github-copilot"]),
    );
    reg.register(
        OpenAiCompatFactory::new(
            "vercel",
            "Vercel AI Gateway",
            crate::aliases::VERCEL_AI_GATEWAY_BASE_URL,
        )
        .aliases(&["vercel-ai"]),
    );
    reg.register(
        OpenAiCompatFactory::new(
            "cloudflare",
            "Cloudflare AI",
            "https://gateway.ai.cloudflare.com/v1",
        )
        .aliases(&["cloudflare-ai"]),
    );

    // ── Azure OpenAI ──
    reg.register(AzureOpenAiFactory);

    // ── China-region providers ──
    reg.register(GlmFactory);
    reg.register(MinimaxFactory);
    reg.register(MoonshotFactory);
    reg.register(QwenFactory);
    reg.register(BailianFactory);
    reg.register(ZaiFactory);
    reg.register(QianfanFactory);
    reg.register(DoubaoFactory);

    // ── Generic providers (base_url required, provider_name used as hint) ──
    reg.register(GenericCompatFactory {
        names: &[
            "sglang",
            "vllm",
            "synthetic",
            "osaurus",
            "deepmyst",
            "deep-myst",
            "ovhcloud",
            "ovh",
            "astrai",
            "avian",
            "aihubmix",
            "siliconflow",
            "silicon-flow",
            "telnyx",
            "openai-codex",
            "openai_codex",
            "codex",
            "kimi-code",
            "kimi_coding",
            "kimi_for_coding",
        ],
    });

    reg
}
