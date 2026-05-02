# clawseed-config — Configuration Loading and Management

## Overview

`clawseed-config` handles TOML configuration file discovery, loading, and environment variable overrides. Configuration structs use the `Configurable` derive macro from `clawseed-macros`.

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
    pub memory: MemoryConfig,
    pub autonomy: AutonomyConfig,
    pub reliability: ReliabilityConfig,
    // ...
}
```

### ProvidersConfig — Provider Configuration

```rust
pub struct ProvidersConfig {
    pub fallback: Option<String>,
    pub models: HashMap<String, ModelConfig>,
}

pub struct ModelConfig {
    pub provider: String,        // "anthropic", "gemini", "openai", etc.
    pub model: String,           // Model name
    pub api_key: Option<String>, // API key (supports ${ENV_VAR} references)
    pub base_url: Option<String>,// Custom endpoint
    pub temperature: Option<f32>,// Temperature
}
```

### AgentConfig — Agent Configuration

```rust
pub struct AgentConfig {
    pub max_tokens: Option<u32>,
    pub cost_limit: Option<f64>,
    pub allowed_tools: Option<Vec<String>>,
}
```

### GatewayConfig — Gateway Configuration

```rust
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub tls: Option<TlsConfig>,
    pub session_backend: SessionBackendConfig,
}
```

### MemoryConfig — Memory Configuration

```rust
pub struct MemoryConfig {
    pub backend: String,           // "sqlite" / "none"
    pub search_mode: SearchMode,   // Hybrid / Embedding / Bm25
    pub embedding: EmbeddingConfig,
}
```

### AutonomyConfig — Autonomy Level Configuration

```rust
pub struct AutonomyConfig {
    pub level: AutonomyLevel,      // ReadOnly / Supervised / Full
    pub allowed_commands: Vec<String>,
    pub max_actions_per_hour: Option<u32>,
}
```

### ReliabilityConfig — Reliability Configuration

```rust
pub struct ReliabilityConfig {
    pub max_retries: usize,
    pub fallback_model: Option<String>,
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
| `CLAWSEED_GATEWAY_HOST` | Gateway listen address |
| `CLAWSEED_GATEWAY_PORT` | Gateway listen port |
| `CLAWSEED_WORKSPACE` | Workspace directory |
| `CLAWSEED_CONFIG_DIR` | Config file directory |

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

## Secret Management

- `secrets.rs` — Secret encryption/decryption
- API keys in config files support `${ENV_VAR}` syntax for referencing environment variables, avoiding plaintext storage
