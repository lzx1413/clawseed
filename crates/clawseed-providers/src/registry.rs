pub struct ProviderInfo {
    /// Canonical name used in config (e.g. `"openrouter"`)
    pub name: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Alternative names accepted in config
    pub aliases: &'static [&'static str],
    /// Whether the provider runs locally (no API key required)
    pub local: bool,
}

/// Return the list of all known providers for display in `clawseed providers list`.
///
/// This is intentionally separate from the factory match in `create_provider`
/// (display concern vs. construction concern).
pub fn list_providers() -> Vec<ProviderInfo> {
    vec![
        // ── Primary providers ────────────────────────────────
        ProviderInfo {
            name: "openrouter",
            display_name: "OpenRouter",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "anthropic",
            display_name: "Anthropic",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "openai",
            display_name: "OpenAI",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "openai-codex",
            display_name: "OpenAI Codex (OAuth)",
            aliases: &["openai_codex", "codex"],
            local: false,
        },
        ProviderInfo {
            name: "telnyx",
            display_name: "Telnyx",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "azure_openai",
            display_name: "Azure OpenAI",
            aliases: &["azure-openai", "azure"],
            local: false,
        },
        ProviderInfo {
            name: "ollama",
            display_name: "Ollama",
            aliases: &[],
            local: true,
        },
        ProviderInfo {
            name: "gemini",
            display_name: "Google Gemini",
            aliases: &["google", "google-gemini"],
            local: false,
        },
        // ── OpenAI-compatible providers ──────────────────────
        ProviderInfo {
            name: "venice",
            display_name: "Venice",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "vercel",
            display_name: "Vercel AI Gateway",
            aliases: &["vercel-ai"],
            local: false,
        },
        ProviderInfo {
            name: "cloudflare",
            display_name: "Cloudflare AI",
            aliases: &["cloudflare-ai"],
            local: false,
        },
        ProviderInfo {
            name: "moonshot",
            display_name: "Moonshot",
            aliases: &["kimi"],
            local: false,
        },
        ProviderInfo {
            name: "kimi-code",
            display_name: "Kimi Code",
            aliases: &["kimi_coding", "kimi_for_coding"],
            local: false,
        },
        ProviderInfo {
            name: "synthetic",
            display_name: "Synthetic",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "opencode",
            display_name: "OpenCode Zen",
            aliases: &["opencode-zen"],
            local: false,
        },
        ProviderInfo {
            name: "opencode-go",
            display_name: "OpenCode Go",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "zai",
            display_name: "Z.AI",
            aliases: &["z.ai"],
            local: false,
        },
        ProviderInfo {
            name: "glm",
            display_name: "GLM (Zhipu)",
            aliases: &["zhipu"],
            local: false,
        },
        ProviderInfo {
            name: "minimax",
            display_name: "MiniMax",
            aliases: &[
                "minimax-intl",
                "minimax-io",
                "minimax-global",
                "minimax-cn",
                "minimaxi",
                "minimax-oauth",
                "minimax-oauth-cn",
                "minimax-portal",
                "minimax-portal-cn",
            ],
            local: false,
        },
        ProviderInfo {
            name: "bedrock",
            display_name: "Amazon Bedrock",
            aliases: &["aws-bedrock"],
            local: false,
        },
        ProviderInfo {
            name: "qianfan",
            display_name: "Qianfan (Baidu)",
            aliases: &["baidu"],
            local: false,
        },
        ProviderInfo {
            name: "doubao",
            display_name: "Doubao (Volcengine)",
            aliases: &["volcengine", "ark", "doubao-cn"],
            local: false,
        },
        ProviderInfo {
            name: "qwen",
            display_name: "Qwen (DashScope / Qwen Code OAuth)",
            aliases: &[
                "dashscope",
                "qwen-intl",
                "dashscope-intl",
                "qwen-us",
                "dashscope-us",
                "qwen-code",
                "qwen-oauth",
                "qwen_oauth",
            ],
            local: false,
        },
        ProviderInfo {
            name: "bailian",
            display_name: "Bailian (Aliyun)",
            aliases: &["aliyun-bailian", "aliyun"],
            local: false,
        },
        ProviderInfo {
            name: "groq",
            display_name: "Groq",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "mistral",
            display_name: "Mistral",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "xai",
            display_name: "xAI (Grok)",
            aliases: &["grok"],
            local: false,
        },
        ProviderInfo {
            name: "deepseek",
            display_name: "DeepSeek",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "together",
            display_name: "Together AI",
            aliases: &["together-ai"],
            local: false,
        },
        ProviderInfo {
            name: "fireworks",
            display_name: "Fireworks AI",
            aliases: &["fireworks-ai"],
            local: false,
        },
        ProviderInfo {
            name: "novita",
            display_name: "Novita AI",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "perplexity",
            display_name: "Perplexity",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "cohere",
            display_name: "Cohere",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "copilot",
            display_name: "GitHub Copilot",
            aliases: &["github-copilot"],
            local: false,
        },
        ProviderInfo {
            name: "claude-code",
            display_name: "Claude Code (CLI)",
            aliases: &[],
            local: true,
        },
        ProviderInfo {
            name: "gemini-cli",
            display_name: "Gemini CLI",
            aliases: &[],
            local: true,
        },
        ProviderInfo {
            name: "kilocli",
            display_name: "KiloCLI",
            aliases: &["kilo"],
            local: true,
        },
        ProviderInfo {
            name: "lmstudio",
            display_name: "LM Studio",
            aliases: &["lm-studio"],
            local: true,
        },
        ProviderInfo {
            name: "llamacpp",
            display_name: "llama.cpp server",
            aliases: &["llama.cpp"],
            local: true,
        },
        ProviderInfo {
            name: "sglang",
            display_name: "SGLang",
            aliases: &[],
            local: true,
        },
        ProviderInfo {
            name: "vllm",
            display_name: "vLLM",
            aliases: &[],
            local: true,
        },
        ProviderInfo {
            name: "osaurus",
            display_name: "Osaurus",
            aliases: &[],
            local: true,
        },
        ProviderInfo {
            name: "nvidia",
            display_name: "NVIDIA NIM",
            aliases: &["nvidia-nim", "build.nvidia.com"],
            local: false,
        },
        ProviderInfo {
            name: "siliconflow",
            display_name: "SiliconFlow",
            aliases: &["silicon-flow"],
            local: false,
        },
        ProviderInfo {
            name: "aihubmix",
            display_name: "AiHubMix",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "litellm",
            display_name: "LiteLLM",
            aliases: &["lite-llm"],
            local: false,
        },
        ProviderInfo {
            name: "mimo",
            display_name: "Mimo",
            aliases: &["xiaomimimo"],
            local: false,
        },
        // ── Fast inference ────────────────────────────────────
        ProviderInfo {
            name: "cerebras",
            display_name: "Cerebras",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "sambanova",
            display_name: "SambaNova",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "hyperbolic",
            display_name: "Hyperbolic",
            aliases: &[],
            local: false,
        },
        // ── Model hosting platforms ──────────────────────────
        ProviderInfo {
            name: "deepinfra",
            display_name: "DeepInfra",
            aliases: &["deep-infra"],
            local: false,
        },
        ProviderInfo {
            name: "huggingface",
            display_name: "Hugging Face",
            aliases: &["hf"],
            local: false,
        },
        ProviderInfo {
            name: "ai21",
            display_name: "AI21 Labs",
            aliases: &["ai21-labs"],
            local: false,
        },
        ProviderInfo {
            name: "reka",
            display_name: "Reka",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "baseten",
            display_name: "Baseten",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "nscale",
            display_name: "Nscale",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "anyscale",
            display_name: "Anyscale",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "nebius",
            display_name: "Nebius AI Studio",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "friendli",
            display_name: "Friendli AI",
            aliases: &["friendliai"],
            local: false,
        },
        ProviderInfo {
            name: "lepton",
            display_name: "Lepton AI",
            aliases: &["lepton-ai"],
            local: false,
        },
        // ── Chinese AI providers ─────────────────────────────
        ProviderInfo {
            name: "stepfun",
            display_name: "Stepfun",
            aliases: &["step"],
            local: false,
        },
        ProviderInfo {
            name: "baichuan",
            display_name: "Baichuan",
            aliases: &[],
            local: false,
        },
        ProviderInfo {
            name: "yi",
            display_name: "01.AI (Yi)",
            aliases: &["01ai", "lingyiwanwu"],
            local: false,
        },
        ProviderInfo {
            name: "hunyuan",
            display_name: "Tencent Hunyuan",
            aliases: &["tencent"],
            local: false,
        },
        // ── Cloud AI endpoints ───────────────────────────────
        ProviderInfo {
            name: "ovhcloud",
            display_name: "OVHcloud AI Endpoints",
            aliases: &["ovh"],
            local: false,
        },
        ProviderInfo {
            name: "avian",
            display_name: "Avian",
            aliases: &[],
            local: false,
        },
    ]
}
