# clawseed-providers — LLM Provider Implementations

## Overview

`clawseed-providers` implements the `Provider` trait for multiple LLM providers, supporting both native protocols and OpenAI-compatible protocols, with retry and fallback mechanisms.

## Supported Providers

### Native Protocol

| Provider | File | Native Tool Calling |
|----------|------|:-------------------:|
| Anthropic | `anthropic.rs` | yes |
| Google Gemini | `gemini.rs` | yes |
| AWS Bedrock | `bedrock.rs` | yes |

### OpenAI-Compatible Protocol

| Provider | File | Auth Style |
|----------|------|------------|
| OpenAI | `compatible.rs` | Bearer Token |
| OpenRouter | `compatible.rs` | Bearer Token |
| Ollama | `compatible.rs` | None |
| DeepSeek | `compatible.rs` | Bearer Token |
| Groq | `compatible.rs` | Bearer Token |
| Mistral | `compatible.rs` | Bearer Token |
| xAI / Grok | `compatible.rs` | Bearer Token |

### China-Region Providers

| Provider | File | Auth Style |
|----------|------|------------|
| GLM (Zhipu) | `factory.rs` | ZhipuJwt |
| MiniMax | `factory.rs` | Bearer Token |
| Moonshot (Kimi) | `factory.rs` | Bearer Token |
| Qwen (Tongyi) | `factory.rs` | Bearer Token |
| Bailian | `factory.rs` | Bearer Token |
| Z.AI | `factory.rs` | ZhipuJwt |
| Qianfan (Baidu) | `factory.rs` | Bearer Token |
| Doubao (Volcengine) | `factory.rs` | Bearer Token |

### Other OpenAI-Compatible Providers

Venice, Together, Fireworks, Perplexity, Cohere, Novita, NVIDIA, GitHub Copilot, Vercel, Cloudflare, Azure OpenAI, and generic compatible endpoints like sglang/vllm.

## Core Modules

### traits.rs — Provider Trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
}
```

### compatible.rs — OpenAI-Compatible Client

A generic client that adapts to different providers through configuration:

```rust
pub struct CompatibleProvider {
    base_url: String,
    auth_style: AuthStyle,
    model: String,
    api_key: Option<String>,
    // ...
}

pub enum AuthStyle {
    Bearer,       // Authorization: Bearer <key>
    XApiKey,      // x-api-key: <key>
    None,         // No auth (e.g., Ollama)
}
```

### reliable.rs — Reliable Provider

Wraps any Provider with retry and fallback:

```rust
pub struct ReliableProvider {
    primary: Box<dyn Provider>,
    fallback: Option<Box<dyn Provider>>,
    max_retries: usize,
}
```

- Retry: Automatically retries on failure, configurable count
- Fallback: Switches to backup provider when primary is unavailable
- Transparent to the agent

### registry.rs — Provider Registry

Look up provider implementations by name.

### factory.rs — Provider Factory

Replaces the previous 300+ line match chain with a `ProviderFactory` trait + `ProviderFactoryRegistry`:

```rust
/// Provider factory trait
pub trait ProviderFactory: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> &[&str] { &[] }
    fn create(&self, provider_name: &str, api_key: Option<&str>,
              base_url: Option<&str>, options: &ProviderRuntimeOptions
    ) -> Result<Box<dyn Provider>>;
}

/// Factory registry
pub struct ProviderFactoryRegistry {
    factories: HashMap<String, Arc<dyn ProviderFactory>>,
}
```

**Built-in factories**:
- `AnthropicFactory` — Native Anthropic protocol
- `GeminiFactory` — Native Gemini protocol
- `BedrockFactory` — Native Bedrock protocol
- `OpenAiCompatFactory` — Parameterized OpenAI-compatible factory; most providers only need name, default URL, and auth style
- Individual China-region factories (GLM, MiniMax, Moonshot, Qwen, Bailian, Z.AI, Qianfan, Doubao)
- `GenericCompatFactory` — Generic compatible endpoints (requires `base_url`)
- `AzureOpenAiFactory` — Azure OpenAI (must provide `base_url`)

**Creation functions**:
```rust
// Uses default registry (LazyLock singleton)
pub fn create_resilient_provider_with_options(
    provider_name: &str, api_key: Option<&str>,
    base_url: Option<&str>, reliability: &ReliabilityConfig,
    options: &ProviderRuntimeOptions,
) -> Result<Box<dyn Provider>>

// Uses a custom registry (Android/embedded scenarios can pass a minimal provider set)
pub fn create_resilient_provider_with_registry(
    registry: &ProviderFactoryRegistry,
    provider_name: &str, api_key: Option<&str>,
    base_url: Option<&str>, reliability: &ReliabilityConfig,
    options: &ProviderRuntimeOptions,
) -> Result<Box<dyn Provider>>
```

### Other Modules

| Module | Responsibility |
|--------|---------------|
| `multimodal.rs` | Image/multimodal support |
| `options.rs` | Provider runtime options |
| `auth/` | OAuth and credential handling |
| `aliases.rs` | Provider name aliases |
| `models_dev.rs` | Development model definitions |

## Token Estimation

Providers estimate token usage from response metadata, used for cost tracking.

## Configuration Example

```toml
[providers]
fallback = "openai"

[providers.models.default]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"

[providers.models.fast]
provider = "groq"
model = "llama-3.1-8b"
api_key = "${GROQ_API_KEY}"

[reliability]
max_retries = 3
fallback_model = "fast"
```
