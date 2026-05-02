# ClawSeed Architecture Overview

## Overview

ClawSeed is an AI agent runtime written in Rust. It connects to LLM providers (Anthropic, Gemini, Bedrock, OpenAI-compatible endpoints, and more), acts through pluggable tools, and serves clients over HTTP/WebSocket.

Core design principle: **runtime, not application**. ClawSeed provides crates that applications assemble — it does not bundle channels, dashboards, or integrations. See [Runtime vs Application](#runtime-vs-application) below.

## Runtime vs Application

An agent **runtime** should do exactly three things: receive messages, call an LLM, and execute tools. Everything else — where messages come from, how results are displayed, which integrations are wired up — belongs to the application layer.

ClawSeed is a runtime. Applications built on it decide:

- How users interact (CLI, mobile app, chat bot, web dashboard)
- Which channels to connect (Discord, Telegram, email — or none)
- Which tools to expose (built-in, remote from mobile, custom)
- How to handle security and approval flows

```toml
# A Discord bot application
[dependencies]
clawseed-agent = "0.7"
clawseed-providers = "0.7"
serenity = "0.12"          # App chooses its own Discord SDK

# An Android application
[dependencies]
clawseed-gateway = "0.7"
clawseed-agent = "0.7"

# A CLI tool
[dependencies]
clawseed-agent = "0.7"
clawseed-tools = "0.7"
```

This is the fundamental architectural split from ZeroClaw. ZeroClaw bundled 40+ channel adapters, hardware peripherals, a TUI, a web dashboard, and an SOP engine into a single binary — making it an application, not a runtime. Adding a new channel meant modifying the runtime. Adding a new integration meant understanding the entire system.

ClawSeed's approach: **the runtime provides crates with stable traits; applications compose them.** When a new need arises, you write a new application — you don't modify the runtime.

## Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                  gateway (REST / WebSocket)               │
│                       ↓                                   │
│  ┌──────────────────────────────────────────────────┐    │
│  │              Agent (stable core)                  │    │
│  │     turn → LLM → dispatch → execute → loop       │    │
│  └──┬──────────┬──────────┬──────────┬─────────────┘    │
│     │          │          │          │                    │
│  provider    tools      memory    hooks                  │
│  (dyn)     (dyn)       (dyn)    (pipeline)               │
│     │          │          │          │                    │
│  Anthropic   25+        SQLite   security                │
│  Gemini      built-in   vector   audit                   │
│  Bedrock                search   approval                │
│  OpenAI*     + remote ──→ mobile client                  │
│  Ollama                                                  │
│  DeepSeek                                                │
│  Groq                                                    │
└──────────────────────────────────────────────────────────┘
   * and any OpenAI-compatible endpoint
```

## Dependency Flow

Dependencies flow one-way, forming a clean layered architecture:

```
clawseed-api (zero deps, trait definitions only)
    ↑
    ├← clawseed-tools       (tool implementations)
    ├← clawseed-memory       (storage backends)
    ├← clawseed-providers    (LLM providers)
    ├← clawseed-parser       (message parsing)
    └← clawseed-agent        (agent core)
            ↑
            └← clawseed-config   (config loading)
                    ↑
                    └← clawseed-gateway (HTTP/WS server + remote tool bridge)
                            ↑
                            └← clawseed (binary entry point)
```

**Key rule**: `clawseed-api` is the only crate with broad dependencies, and it depends on no other crate. Core never imports extensions.

## Core Abstractions

All extension points in ClawSeed are traits:

| Trait | Purpose | How to extend |
|-------|---------|---------------|
| `Provider` | LLM inference backend | Implement in `clawseed-providers`, or register a custom `ProviderFactory` |
| `Tool` | Agent-callable capability | Implement in `clawseed-tools`, or register remote tools via WebSocket |
| `ToolRegistry` | Unified tool registration and lookup | `DefaultToolRegistry` in `clawseed-agent`; supports BuiltIn / MCP / Remote sources |
| `Hook` | Tool call interceptor | Implement `before_tool_call` / `after_tool_call`, or create declaratively via `HookFactory` from config |
| `Memory` | Conversation memory backend | Implement in `clawseed-memory` |
| `Observer` | Metrics and tracing | Implement `on_event()` |
| `ContextProvider` | Capability injection | Inject any `Send + Sync + 'static` type into the agent |

## Agent Loop

The agent's core is a turn loop, triggered by each user message:

```
User message
  ↓
Build system prompt (prompt.rs)
  ↓
Call LLM (Provider::chat())
  ↓
Parse response (ToolDispatcher::parse_response())
  ├── Text-only response → return to user
  └── Contains tool calls → enter tool loop
        ↓
  For each tool call:
    1. before_hook interception (can cancel/modify)
    2. Tool::execute()
    3. after_hook observation
        ↓
  Format tool results, send back to LLM
        ↓
  Return to "parse response" step until LLM returns text-only
```

## Remote Tool Calls

Mobile clients register tools over WebSocket. The gateway wraps each spec as a `RemoteTool` (implementing the `Tool` trait). The agent has no branching for remote vs. local tools:

```
┌──────────────┐     register_tools       ┌──────────────┐
│   Mobile     │ ───────────────────────→ │   Gateway    │
│   Client     │                          │              │
│              │ ←── tool_call_request ── │   Agent      │
│  (executes   │ ──── tool_result ──────→ │   calls it   │
│   on device) │                          │   like any   │
│              │ ←── result_acknowledged─ │   other tool │
└──────────────┘                          └──────────────┘
```

## Capability Injection

Tools don't receive dependencies through constructors. Instead, they look them up at runtime via `ToolContext`:

```rust
// Inject at construction time
agent_builder.capability(Arc::new(my_service));

// Look up at execution time
if let Some(svc) = ctx.get::<MyService>() {
    svc.do_thing();
}
```

Under the hood, this uses a `TypeId` → `Arc<dyn Any>` map, requiring no generic parameters and decoupling tool traits from extension types.

## Tool Registry

The Agent manages all tool sources through the `ToolRegistry` trait (defined in `clawseed-api`):

```rust
// Three tool sources
pub enum ToolSource {
    BuiltIn,                        // Built-in tools
    Mcp { server: String },         // MCP server tools
    Remote { session: String },     // Remote client tools (e.g., Android)
}

// Registration and lookup
registry.register(tool, ToolSource::BuiltIn);
registry.register_or_replace(tool, ToolSource::Remote { session });
let tool = registry.get_tool("shell");
let specs = registry.tool_specs();  // Cached ToolSpec list
```

`DefaultToolRegistry` (in `clawseed-agent`) uses `DashMap` for lock-free concurrent access, with glob pattern-based tool filtering (`allowed_tools` / `denied_tools`) and per-MCP-server filtering.

## Provider Factory

Providers register through the `ProviderFactory` trait + `ProviderFactoryRegistry`:

```rust
// Custom provider factory
impl ProviderFactory for MyFactory {
    fn name(&self) -> &str { "my-provider" }
    fn aliases(&self) -> &[&str] { &["my-alias"] }
    fn create(&self, name: &str, api_key: Option<&str>,
              base_url: Option<&str>, options: &ProviderRuntimeOptions
    ) -> Result<Box<dyn Provider>> { /* ... */ }
}

// Register in the registry
let mut reg = ProviderFactoryRegistry::new();
reg.register(MyFactory);

// Create Agent with a custom registry
Agent::from_config_with_registry(&config, Some(Arc::new(reg))).await?;
```

Replaces the previous 300+ line match chain. Android/embedded scenarios can pass a minimal provider set.

## Security Model

- **Autonomy levels**: `ReadOnly` / `Supervised` / `Full`
- **SecurityPolicy**: Injected as a Hook — implements the `Hook` trait to globally intercept tool calls before execution (checking autonomy level, rate limits, command allowlists, path guards); always the first hook in the pipeline
- **Command allowlists**: `allowed_commands` validates shell commands
- **Path guards**: Blocks access to sensitive paths (`/etc/passwd`, `/root/.ssh`, etc.)
- **Rate limiting**: `max_actions_per_hour` limits actions per session
- **Hook pipeline**: `Hook::before_tool_call()` can cancel or modify any tool call; SecurityPolicy is always the first hook in the pipeline
- **Tool filtering**: `allowed_tools` / `denied_tools` glob patterns, `mcp_tool_filters` per MCP server

## Design Principles

1. **Explicit over implicit** — `all_tools()` lists every tool; the full capability set is visible at a glance
2. **Declarative over imperative** — Config drives composition, not code changes
3. **Traits at boundaries** — Core depends on abstractions; implementations live outside
4. **Graceful degradation** — Missing capability → tool skips the feature; failed memory → NoneMemory fallback; flaky provider → ReliableProvider retries

## Crate Overview

| Crate | Role | Depends on api | Depends on agent |
|-------|------|:--------------:|:----------------:|
| `clawseed-api` | Trait definitions only | — | — |
| `clawseed-agent` | Agent loop, hooks, dispatch | yes | — |
| `clawseed-tools` | 25+ built-in tools | yes | no |
| `clawseed-providers` | LLM provider implementations | yes | no |
| `clawseed-memory` | SQLite-backed memory + vector search | yes | no |
| `clawseed-config` | TOML config schema and loading | yes | no |
| `clawseed-parser` | Tool call parsing | yes | no |
| `clawseed-macros` | Procedural macros | no | no |
| `clawseed-gateway` | Axum HTTP/WS server + remote tool bridge | yes | yes |
| `clawseed` | Binary (CLI) | — | — |
