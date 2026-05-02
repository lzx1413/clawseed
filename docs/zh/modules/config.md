# clawseed-config — 配置加载与管理

## 概述

`clawseed-config` 负责 TOML 配置文件的发现、加载和环境变量覆盖。配置结构使用 `clawseed-macros` 的 `Configurable` 派生宏。

## 配置发现

配置文件搜索顺序（优先级从高到低）：

1. `$CLAWSEED_CONFIG_DIR/clawseed.toml`（环境变量指定目录）
2. `~/.clawseed/clawseed.toml`（默认配置目录）
3. `./clawseed.toml`（当前工作目录）

首次运行时自动创建 `~/.clawseed/` 目录。

## 默认路径

| 路径 | 函数 | 默认值 |
|------|------|--------|
| 配置目录 | `default_config_dir()` | `~/.clawseed/` |
| 工作目录 | `default_workspace_dir()` | `~/.clawseed/workspace/` |

## 配置 Schema

### 顶层 Config

```rust
pub struct Config {
    pub providers: ProvidersConfig,
    pub agent: AgentConfig,
    pub gateway: GatewayConfig,
    pub memory: MemoryConfig,
    pub autonomy: AutonomyConfig,
    pub reliability: ReliabilityConfig,
    // ...
}
```

### ProvidersConfig — 提供商配置

```rust
pub struct ProvidersConfig {
    pub fallback: Option<String>,
    pub models: HashMap<String, ModelConfig>,
}

pub struct ModelConfig {
    pub provider: String,        // "anthropic", "gemini", "openai", etc.
    pub model: String,           // 模型名称
    pub api_key: Option<String>, // API 密钥（支持 ${ENV_VAR} 引用）
    pub base_url: Option<String>,// 自定义端点
    pub temperature: Option<f32>,// 温度
}
```

### AgentConfig — Agent 配置

```rust
pub struct AgentConfig {
    pub max_tokens: Option<u32>,
    pub cost_limit: Option<f64>,
    pub allowed_tools: Option<Vec<String>>,
}
```

### GatewayConfig — 网关配置

```rust
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub tls: Option<TlsConfig>,
    pub session_backend: SessionBackendConfig,
}
```

### MemoryConfig — 记忆配置

```rust
pub struct MemoryConfig {
    pub backend: String,           // "sqlite" / "none"
    pub search_mode: SearchMode,   // Hybrid / Embedding / Bm25
    pub embedding: EmbeddingConfig,
}
```

### AutonomyConfig — 自主等级配置

```rust
pub struct AutonomyConfig {
    pub level: AutonomyLevel,      // ReadOnly / Supervised / Full
    pub allowed_commands: Vec<String>,
    pub max_actions_per_hour: Option<u32>,
}
```

### ReliabilityConfig — 可靠性配置

```rust
pub struct ReliabilityConfig {
    pub max_retries: usize,
    pub fallback_model: Option<String>,
}
```

## 环境变量覆盖

环境变量优先级高于配置文件：

| 环境变量 | 对应配置项 |
|----------|-----------|
| `CLAWSEED_PROVIDER` | 默认提供商 |
| `CLAWSEED_MODEL` | 默认模型 |
| `CLAWSEED_API_KEY` | API 密钥 |
| `CLAWSEED_PROVIDER_URL` | 提供商 URL |
| `CLAWSEED_GATEWAY_HOST` | 网关监听地址 |
| `CLAWSEED_GATEWAY_PORT` | 网关监听端口 |
| `CLAWSEED_WORKSPACE` | 工作目录 |
| `CLAWSEED_CONFIG_DIR` | 配置文件目录 |

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

[agent]
max_tokens = 4096
allowed_tools = ["file_read", "file_write", "shell", "memory_store", "memory_recall"]

[gateway]
host = "0.0.0.0"
port = 3000

[memory]
backend = "sqlite"
search_mode = "hybrid"

[autonomy]
level = "supervised"
allowed_commands = ["ls", "cat", "grep", "find", "git"]
max_actions_per_hour = 100

[reliability]
max_retries = 3
fallback_model = "fast"
```

## 密钥管理

- `secrets.rs` — 密钥加密/解密
- 配置文件中的 API 密钥支持 `${ENV_VAR}` 语法引用环境变量，避免明文存储
