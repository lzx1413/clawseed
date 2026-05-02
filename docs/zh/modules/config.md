# clawseed-config — 配置加载与管理

## 概述

`clawseed-config` 负责 TOML 配置文件的发现、加载和环境变量覆盖。

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
    pub storage: StorageConfig,
    pub reliability: ReliabilityConfig,
    pub secrets: SecretsConfig,
    pub runtime: RuntimeConfig,
    pub memory: MemoryConfig,
    pub autonomy: AutonomyConfig,
    pub cron: CronConfig,
    pub scheduler: SchedulerConfig,
    pub backup: BackupConfig,
    pub composio: ComposioConfig,
    pub mcp: McpConfig,
    pub hooks: HooksConfig,
    pub tunnel: TunnelConfig,
    pub nodes: NodesConfig,
    pub cost: CostConfig,
    pub channels: ChannelsConfig,
    pub browser: BrowserConfig,
    pub http_request: HttpRequestConfig,
    pub web_fetch: WebFetchConfig,
    pub transcription: TranscriptionConfig,
    pub web_search: WebSearchConfig,
    pub agents: HashMap<String, AgentEntryConfig>,
    pub locale: Option<String>,
    // ...
}
```

### ProvidersConfig — 提供商配置

```rust
pub struct ProvidersConfig {
    pub default: Option<String>,
    pub default_model: Option<String>,
    pub default_api_key: Option<String>,
    pub default_base_url: Option<String>,
    pub default_timeout_secs: Option<u64>,
    pub extra_headers: HashMap<String, String>,
    pub models: HashMap<String, ModelProviderConfig>,
    pub model_routes: Vec<ModelRouteConfig>,
    pub embedding_routes: Vec<EmbeddingRouteConfig>,
    pub fallback: Option<String>,
    pub reasoning_effort: Option<String>,
    pub reasoning_enabled: Option<bool>,
}

pub struct ModelProviderConfig {
    pub api_key: Option<String>,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_path: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub timeout_secs: Option<u64>,
    pub extra_headers: HashMap<String, String>,
    pub wire_api: Option<String>,
    pub max_tokens: Option<u32>,
    pub provider_extra: Option<serde_json::Value>,
    pub merge_system_into_user: bool,
}
```

### AgentConfig — Agent 配置

```rust
pub struct AgentConfig {
    pub max_tool_iterations: usize,           // 最大工具循环次数（默认 25）
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub web_search_enabled: bool,
    pub web_search_provider: Option<String>,
    pub system_prompt: Option<String>,
    pub memory_namespace: Option<String>,
    pub daily_budget_usd: Option<f64>,
    pub turn_budget_usd: Option<f64>,
    pub allowed_tools: Vec<String>,           // glob 模式工具白名单（空 = 允许全部）
    pub denied_tools: Vec<String>,            // glob 模式工具黑名单（优先于白名单）
    pub mcp_tool_filters: HashMap<String, Vec<String>>,  // MCP 服务器级工具过滤
}
```

### GatewayConfig — 网关配置

```rust
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub timeout_secs: u64,
    pub path_prefix: Option<String>,
    pub session_persistence: bool,
    pub session_ttl_hours: u32,
    pub tls: Option<GatewayTlsConfig>,
    pub enable_cors: bool,
    pub require_pairing: bool,
    pub paired_tokens: Vec<String>,
    pub allow_public_bind: bool,
    pub pair_rate_limit_per_minute: u32,
    pub webhook_rate_limit_per_minute: u32,
    pub rate_limit_max_keys: usize,
    pub idempotency_ttl_secs: u64,
    pub idempotency_max_keys: usize,
    pub trust_forwarded_headers: bool,
    pub web_dist_dir: Option<String>,
    pub pairing_dashboard: PairingDashboardConfig,
}
```

### MemoryConfig — 记忆配置

```rust
pub struct MemoryConfig {
    pub backend: String,           // "sqlite" / "none"
    pub auto_save: bool,
    pub min_relevance_score: f64,
    pub response_cache_enabled: bool,
    pub response_cache_ttl_minutes: u64,
    pub response_cache_max_entries: usize,
    pub response_cache_hot_entries: usize,
    pub namespace: Option<String>,
    pub qdrant: QdrantConfig,
}
```

### AutonomyConfig — 自主等级配置

```rust
pub struct AutonomyConfig {
    pub level: AutonomyLevel,      // ReadOnly / Supervised / Full
    pub auto_approve: Vec<String>,
    pub always_ask: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub non_cli_excluded_tools: Vec<String>,
    pub max_actions_per_hour: u32,
}
```

### ReliabilityConfig — 可靠性配置

```rust
pub struct ReliabilityConfig {
    pub max_retries: u32,
    pub retry_delay_secs: u64,
    pub scheduler_poll_secs: u64,
    pub enable_fallback: bool,
    pub scheduler_retries: u32,
    pub provider_backoff_ms: u64,
    pub api_keys: Vec<String>,
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
| `CLAWSEED_PROVIDER_TIMEOUT_SECS` | 提供商超时 |
| `CLAWSEED_GATEWAY_HOST` | 网关监听地址 |
| `CLAWSEED_GATEWAY_PORT` | 网关监听端口 |
| `CLAWSEED_WORKSPACE` | 工作目录 |
| `CLAWSEED_EXTRA_HEADERS` | 额外 HTTP 头（逗号分隔 key=value） |
| `CLAWSEED_TEMPERATURE` | 采样温度 |
| `CLAWSEED_STORAGE_DB_URL` | 存储 DB URL |
| `CLAWSEED_WEB_SEARCH_ENABLED` | 启用网页搜索 |
| `CLAWSEED_WEB_SEARCH_PROVIDER` | 网页搜索提供商 |

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
max_tool_iterations = 25
allowed_tools = ["file_*", "memory_*", "shell"]
denied_tools = ["dangerous_tool"]

[agent.mcp_tool_filters]
my_mcp_server = ["search_*", "read_*"]

[gateway]
host = "0.0.0.0"
port = 3000

[memory]
backend = "sqlite"
auto_save = true

[autonomy]
level = "supervised"
allowed_commands = ["ls", "cat", "grep", "find", "git"]
max_actions_per_hour = 100

[reliability]
max_retries = 2
provider_backoff_ms = 500

[secrets]
encrypt = true

[hooks]
enabled = true

[[hooks.chain]]
type = "security_policy"

[[hooks.chain]]
type = "audit_log"
config = { level = "info" }
```

## 密钥管理

- `secrets.rs` — 密钥加密/解密
- 配置文件中的 API 密钥支持 `${ENV_VAR}` 语法引用环境变量，避免明文存储
