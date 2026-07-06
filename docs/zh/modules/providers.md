# clawseed-providers — LLM 提供商实现

## 概述

`clawseed-providers` 实现了多种 LLM 提供商的 `Provider` trait，支持原生协议和 OpenAI 兼容协议，并提供重试和回退机制。

## 支持的提供商

### 原生协议

| 提供商 | 文件 | 原生工具调用 | 缓存策略 |
|--------|------|:----------:|:--------:|
| Anthropic | `anthropic.rs` | yes | ExplicitAnthropic |
| Google Gemini | `gemini.rs` | yes | None (自动前缀缓存) |
| AWS Bedrock | `bedrock.rs` | yes | ExplicitAnthropic |
| DeepSeek (Anthropic API) | `factory.rs` → `anthropic.rs` | yes | ExplicitAnthropic |

### OpenAI 兼容协议

| 提供商 | 文件 | 认证方式 | 缓存策略 |
|--------|------|---------|:--------:|
| OpenAI | `compatible.rs` | Bearer Token | None (自动前缀缓存) |
| OpenRouter | `compatible.rs` | Bearer Token | None |
| Ollama | `compatible.rs` | 无认证 | None |
| DeepSeek | `compatible.rs` | Bearer Token | None (自动前缀缓存) |
| Groq | `compatible.rs` | Bearer Token | None (自动前缀缓存) |
| Mistral | `compatible.rs` | Bearer Token | None |
| xAI / Grok | `compatible.rs` | Bearer Token | None |

### 中国区提供商

| 提供商 | 文件 | 认证方式 |
|--------|------|---------|
| GLM (智谱) | `factory.rs` | ZhipuJwt |
| MiniMax | `factory.rs` | Bearer Token |
| Moonshot (Kimi) | `factory.rs` | Bearer Token |
| Qwen (通义千问) | `factory.rs` | Bearer Token |
| Bailian (百炼) | `factory.rs` | Bearer Token |
| Z.AI | `factory.rs` | ZhipuJwt |
| Qianfan (千帆) | `factory.rs` | Bearer Token |
| Doubao (豆包) | `factory.rs` | Bearer Token |

### 其他 OpenAI 兼容提供商

Venice、Together、Fireworks、Perplexity、Cohere、Novita、NVIDIA、GitHub Copilot、Vercel、Cloudflare、Azure OpenAI，以及 sglang/vllm 等通用兼容端点。

## 核心模块

### Provider trait（定义在 clawseed-api）

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat_with_system(&self, system_prompt: Option<&str>, message: &str, model: &str, temperature: Option<f64>) -> Result<String>;
    async fn chat(&self, request: ChatRequest<'_>, model: &str, temperature: Option<f64>) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
    fn stream_chat(&self, request: ChatRequest<'_>, model: &str, temperature: Option<f64>, options: StreamOptions) -> BoxStream<'static, StreamResult<StreamEvent>>;
    // ... 更多带默认实现的方法
}
```

### compatible/mod.rs — OpenAI 兼容客户端

通用客户端，通过配置适配不同提供商：

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
    None,         // 无认证（如 Ollama）
}
```

### reliable.rs — 可靠提供者

包装任意 Provider，添加重试和回退：

```rust
pub struct ReliableProvider {
    primary: Box<dyn Provider>,
    fallback: Option<Box<dyn Provider>>,
    max_retries: usize,
}
```

- 重试：失败后自动重试，可配置次数
- 回退：主提供者不可用时切换到备用提供者
- 对 Agent 透明

### registry.rs — 提供者注册表

按名称查找提供者实现。

### factory.rs — 提供者工厂

通过 `ProviderFactory` trait + `ProviderFactoryRegistry` 替代了原来 300+ 行的 match 链：

```rust
/// 提供者工厂 trait
pub trait ProviderFactory: Send + Sync {
    fn name(&self) -> &str;
    fn aliases(&self) -> &[&str] { &[] }
    fn create(&self, provider_name: &str, api_key: Option<&str>,
              base_url: Option<&str>, options: &ProviderRuntimeOptions
    ) -> Result<Box<dyn Provider>>;
}

/// 工厂注册表
pub struct ProviderFactoryRegistry {
    factories: HashMap<String, Arc<dyn ProviderFactory>>,
}
```

**内置工厂**：
- `AnthropicFactory` — 原生 Anthropic 协议，支持 `cache_control: ephemeral`
- `GeminiFactory` — 原生 Gemini 协议
- `BedrockFactory` — 原生 Bedrock 协议，支持 `CachePoint`
- `DeepSeekAnthropicFactory` — DeepSeek Anthropic 兼容端点 (`deepseek-anthropic` / `deepseek-claude`)。使用 `AnthropicProvider::with_base_url()` 包装 DeepSeek `/anthropic` URL，提供完整的 `cache_control: ephemeral` 支持
- `OpenAiCompatFactory` — 参数化的 OpenAI 兼容工厂，大多数提供商仅需提供名称、默认 URL 和认证方式
- 中国区各厂商独立工厂（GLM、MiniMax、Moonshot、Qwen、Bailian、Z.AI、Qianfan、Doubao）
- `GenericCompatFactory` — 通用兼容端点（需要 `base_url`）
- `AzureOpenAiFactory` — Azure OpenAI（必须提供 `base_url`）

**创建函数**：
```rust
// 使用默认注册表（LazyLock 单例）
pub fn create_resilient_provider_with_options(
    provider_name: &str, api_key: Option<&str>,
    base_url: Option<&str>, reliability: &ReliabilityConfig,
    options: &ProviderRuntimeOptions,
) -> Result<Box<dyn Provider>>

// 使用自定义注册表（Android/嵌入式场景可传入最小化的 provider 集合）
pub fn create_resilient_provider_with_registry(
    registry: &ProviderFactoryRegistry,
    provider_name: &str, api_key: Option<&str>,
    base_url: Option<&str>, reliability: &ReliabilityConfig,
    options: &ProviderRuntimeOptions,
) -> Result<Box<dyn Provider>>
```

### 其他模块

| 模块 | 职责 |
|------|------|
| `multimodal.rs` | 图片/多模态支持 |
| `options.rs` | 提供者运行时选项 |
| `auth/` | OAuth 和凭证处理 |
| `aliases.rs` | 提供者名称别名 |
| `models_dev.rs` | 开发用模型定义 |

## 令牌估算

提供者根据响应元数据估算令牌使用量，用于成本追踪。`TokenUsage.cached_input_tokens` 从提供商特定字段填充：

- **DeepSeek** (`/v1/chat/completions`)：`prompt_cache_hit_tokens` — 报告前缀缓存的输入 tokens
- **OpenAI**：`prompt_tokens_details.cached_tokens` — 嵌套的缓存 token 计数
- **Anthropic / Bedrock**：`cache_read_input_tokens` 来自 Anthropic 响应格式
- 提取逻辑在 `UsageInfo::extract_cached_tokens()` 中，先尝试 DeepSeek 字段，然后回退到 OpenAI 嵌套字段

## 配置示例

```toml
[providers]
fallback = "anthropic"

[providers.models.anthropic]
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"

[providers.models.groq]
model = "llama-3.1-8b"
api_key = "${GROQ_API_KEY}"

[reliability]
max_retries = 3
provider_backoff_ms = 500
```
