# clawseed-providers — LLM 提供商实现

## 概述

`clawseed-providers` 实现了多种 LLM 提供商的 `Provider` trait，支持原生协议和 OpenAI 兼容协议，并提供重试和回退机制。

## 支持的提供商

### 原生协议

| 提供商 | 文件 | 原生工具调用 |
|--------|------|:----------:|
| Anthropic | `anthropic.rs` | yes |
| Google Gemini | `gemini.rs` | yes |
| AWS Bedrock | `bedrock.rs` | yes |

### OpenAI 兼容协议

| 提供商 | 文件 | 认证方式 |
|--------|------|---------|
| OpenAI | `compatible.rs` | Bearer Token |
| OpenRouter | `compatible.rs` | Bearer Token |
| Ollama | `compatible.rs` | 无认证 |
| DeepSeek | `compatible.rs` | Bearer Token |
| Groq | `compatible.rs` | Bearer Token |
| Mistral | `compatible.rs`` | Bearer Token |
| xAI / Grok | `compatible.rs` | Bearer Token |

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
- `AnthropicFactory` — 原生 Anthropic 协议
- `GeminiFactory` — 原生 Gemini 协议
- `BedrockFactory` — 原生 Bedrock 协议
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

提供者根据响应元数据估算令牌使用量，用于成本追踪。

## 配置示例

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
