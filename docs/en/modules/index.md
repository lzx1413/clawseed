# Modules

ClawSeed is organized as a Cargo workspace of crates with clean dependency flow.

| Crate | Role |
|-------|------|
| [clawseed-api](api.md) | Core trait definitions only |
| [clawseed-agent](agent.md) | Agent loop, hooks, dispatch, runtime assembly |
| [clawseed-tools](tools.md) | 25+ built-in tool implementations |
| [clawseed-providers](providers.md) | LLM provider implementations |
| [clawseed-memory](memory.md) | SQLite-backed memory + vector search |
| [clawseed-config](config.md) | TOML config schema and loading |
| [clawseed-gateway](gateway.md) | Axum HTTP/WS server + remote tool bridge |
