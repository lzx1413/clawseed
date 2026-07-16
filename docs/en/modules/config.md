# clawseed-config — Configuration Loading and Management

## Overview

`clawseed-config` handles TOML configuration file discovery, loading, and environment variable overrides.

## Config Discovery

Configuration file search order (highest to lowest priority):

1. `$CLAWSEED_CONFIG_DIR/clawseed.toml` (environment variable directory)
2. `~/.clawseed/clawseed.toml` (default config directory)
3. `./clawseed.toml` (current working directory)

The `~/.clawseed/` directory is auto-created on first run.

## Default Paths

| Path | Function | Default |
|------|----------|---------|
| Config directory | `default_config_dir()` | `~/.clawseed/` |
| Workspace directory | `default_workspace_dir()` | `~/.clawseed/workspace/` |

## Configuration Schema

### Top-Level Config

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
    pub user_model: UserModelConfig,
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
    pub identity: IdentityConfig,
    pub locale: Option<String>,
    // ...
}
```

### ProvidersConfig — Provider Configuration

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

### AgentConfig — Agent Configuration

```rust
pub struct AgentConfig {
    pub max_tool_iterations: usize,           // Max tool loop iterations (default 25)
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub auto_continue_on_truncation: bool,
    pub max_auto_continue: usize,
    pub web_search_enabled: bool,
    pub web_search_provider: Option<String>,
    pub system_prompt: Option<String>,
    pub memory_namespace: Option<String>,
    pub daily_budget_usd: Option<f64>,
    pub turn_budget_usd: Option<f64>,
    pub allowed_tools: Vec<String>,           // Glob pattern tool allowlist (empty = allow all)
    pub denied_tools: Vec<String>,            // Glob pattern tool denylist (takes precedence)
    pub mcp_tool_filters: HashMap<String, Vec<String>>,  // Per-MCP-server tool filtering
}
```

### GatewayConfig — Gateway Configuration

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

### MemoryConfig — Memory Configuration

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
    pub hygiene_enabled: bool,
    pub conversation_retention_days: u32,
    pub snapshot_enabled: bool,
    pub auto_hydrate: bool,
    pub conflict_threshold: f64,
    pub conflict_mode: Option<ConflictMode>,
    pub auto_recall: bool,
    pub auto_recall_limit: usize,
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_dims: Option<usize>,
    pub search_mode: Option<SearchMode>,
    pub vector_weight: Option<f32>,
    pub keyword_weight: Option<f32>,
    pub merge_strategy: Option<MergeStrategy>,
    pub embedding_cache_max: Option<usize>,
    pub backfill_on_startup: bool,
    pub defer_embedding: Option<bool>,
    pub stable_memory_in_system_prompt: Option<bool>,
    pub min_retention_floor: Option<usize>,
    pub daily_retention_floor: Option<usize>,
    pub conversation_retention_floor: Option<usize>,
}
```

### AutonomyConfig — Autonomy Level Configuration

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

### ReliabilityConfig — Reliability Configuration

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

## Environment Variable Overrides

Environment variables take precedence over the config file:

| Variable | Config Field |
|----------|-------------|
| `CLAWSEED_PROVIDER` | Default provider |
| `CLAWSEED_MODEL` | Default model |
| `CLAWSEED_API_KEY` | API key |
| `CLAWSEED_PROVIDER_URL` | Provider URL |
| `CLAWSEED_PROVIDER_TIMEOUT_SECS` | Provider timeout |
| `CLAWSEED_GATEWAY_HOST` | Gateway listen address |
| `CLAWSEED_GATEWAY_PORT` | Gateway listen port |
| `CLAWSEED_WORKSPACE` | Workspace directory |
| `CLAWSEED_EXTRA_HEADERS` | Extra HTTP headers (comma-separated key=value) |
| `CLAWSEED_TEMPERATURE` | Sampling temperature |
| `CLAWSEED_STORAGE_DB_URL` | Storage DB URL |
| `CLAWSEED_WEB_SEARCH_ENABLED` | Enable web search |
| `CLAWSEED_WEB_SEARCH_PROVIDER` | Web search provider |
| `CLAWSEED_WEB_SEARCH_TAVILY_API_KEY` | Tavily API key |
| `CLAWSEED_EMBEDDING_PROVIDER` | Memory embedding provider |
| `CLAWSEED_EMBEDDING_MODEL` | Memory embedding model |
| `CLAWSEED_EMBEDDING_API_KEY` | First embedding route API key |
| `CLAWSEED_EMBEDDING_DIMENSIONS` | Memory embedding dimensions |

## Configuration Example

```toml
workspace_dir = "/home/user/workspace"

[providers]
fallback = "anthropic"

[providers.models.anthropic]
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"

[providers.models.groq]
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

[user_model]
enabled = true
max_prompt_items = 20
auto_infer = false
inference_min_confidence = 0.8
max_inferred_items_per_turn = 3

[autonomy]
level = "supervised"
allowed_commands = ["ls", "cat", "grep", "find", "git"]
max_actions_per_hour = 100

[reliability]
max_retries = 2
provider_backoff_ms = 500

[identity]
format = "openclaw"
# format = "aieos"
# aieos_path = "identity.json"

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

`user_model.enabled` controls structured local-user profiles. Profile data is stored in
`<workspace>/user_model/profiles.db`; `max_prompt_items` limits the active, unexpired
items injected into the Agent system prompt. `auto_infer` is an opt-in switch that runs
low-temperature profile extraction after successful turns without delaying the response.
Only non-sensitive items meeting `inference_min_confidence` are accepted, with at most
`max_inferred_items_per_turn` writes per turn. Explicit, imported, and rejected items are
never overwritten by inference.

### IdentityConfig — Identity Configuration

```rust
pub struct IdentityConfig {
    pub format: String,               // "openclaw" (default) or "aieos"
    pub aieos_path: Option<String>,   // Path to AIEOS JSON file (relative to workspace)
    pub aieos_inline: Option<String>, // Inline AIEOS JSON string
}
```

OpenClaw mode (default) uses markdown files in the workspace directory (`SOUL.md`, `IDENTITY.md`, etc.). AIEOS mode uses structured JSON identity. See [Personality & Identity Tutorial](../tutorials/personality-and-identity.md) for details.

## Secret Management

- `secrets.rs` — Secret encryption/decryption
- API keys in config files support `${ENV_VAR}` syntax for referencing environment variables, avoiding plaintext storage
