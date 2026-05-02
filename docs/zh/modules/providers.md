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
| GLM (智谱) | `compatible.rs` | Bearer Token |
| MiniMax | `compatible.rs` | Bearer Token |
| Moonshot | `compatible.rs` | Bearer Token |

## 核心模块

### traits.rs — Provider trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
}
```

### compatible.rs — OpenAI 兼容客户端

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

### factory — 创建函数

```rust
pub fn create_resilient_provider_with_options(config: &Config) -> Result<Box<dyn Provider>>
```

根据配置创建 `ReliableProvider`，自动设置主/备提供者。

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
