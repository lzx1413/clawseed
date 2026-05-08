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
    └← clawseed-agent        (agent core + runtime assembly)
            ↑
            └← clawseed-config   (config loading)
                    ↑
                    └← clawseed-gateway (HTTP/WS server + remote tool bridge)
                            ↑
                            └← clawseed (binary entry point)
```

**Key rule**: `clawseed-api` is the only crate with broad dependencies, and it depends on no other crate. Core never imports extensions.

> **Note:** The arrows above show crate-level import direction. At runtime, `Agent::from_config_with_registry()` directly instantiates provider, memory, and tools from their respective crates — the agent crate is not a pure orchestration layer, it also owns runtime assembly. The gateway uses `Agent::from_config_with_shared_components()` to reuse shared `AppState` components (provider, memory, observer) across connections instead of creating new ones per connection.

## Core Abstractions

All extension points in ClawSeed are traits:

| Trait | Purpose | How to extend |
|-------|---------|---------------|
| `Provider` | LLM inference backend | Implement in `clawseed-providers`, or register a custom `ProviderFactory` |
| `Tool` | Agent-callable capability | Implement in `clawseed-tools`, or register remote tools via WebSocket |
| `ToolRegistry` | Unified tool registration and lookup | `DefaultToolRegistry` in `clawseed-agent`; supports BuiltIn / MCP / Remote sources. MCP is defined in the enum and registry infrastructure supports it, but the actual MCP protocol client is not yet implemented — see "MCP Status" below |
| `Hook` | Tool call interceptor | Implement `before_tool_call` / `after_tool_call`, or create declaratively via `HookFactory` from config |
| `Memory` | Conversation memory backend | Implement in `clawseed-memory` |

## Agent Assembly & Loop

`Agent::from_config_with_registry()` is the primary constructor for CLI/embedded use. It does runtime assembly — directly instantiates provider (via `ProviderFactoryRegistry`), memory (via `clawseed_memory::create_memory()`), and tools (via `clawseed_tools::registry::all_tools()`), then selects a dispatcher based on `provider.supports_native_tools()`. Tools depend on memory being constructed first; dispatcher depends on provider capabilities. All components are passed to `Agent::builder()` for final construction.

`Agent::from_config_with_shared_components()` is the constructor for gateway use. It accepts pre-built `Arc<dyn Provider>`, `Arc<dyn Memory>`, `Arc<dyn Observer>`, model name, and temperature from `AppState` — these shared components are reused across all WebSocket/webhook connections. All other assembly logic (tools, hooks, filters, skills, identity, etc.) is shared via the private `build_from_config()` helper to avoid duplication. The provider field is `Arc<dyn Provider>` (not `Box`); `AgentBuilder.provider()` wraps `Box→Arc`, and `shared_provider()` accepts `Arc` directly.

The agent's core is a turn loop, triggered by each user message:

```
User message
  ↓
Build system prompt (prompt.rs)
  ↓
Call LLM (Provider::chat())
  ↓
Parse response (ToolDispatcher::parse_response())
├── NativeToolDispatcher: extract directly from provider's native tool_calls
└── XmlToolDispatcher: try ◁▷ format first, fallback to multi-format parser (12+ formats)
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

Mobile clients register tools over WebSocket. The gateway wraps each spec as a `RemoteTool` (implementing the `Tool` trait). Remote tool registration is a three-step flow:

1. **Register to shared registry** — `state.tool_registry.register_or_replace(tool, ToolSource::Remote { session })` so `/api/tools` reflects the tool globally
2. **Inject into per-connection Agent** — `agent.add_remote_tools(tools, session)` before processing each message
3. **Cleanup on disconnect** — `state.tool_registry.unregister_by_source(&ToolSource::Remote { session })`

The agent has no branching for remote vs. local tools:

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

## Dual Tool Registry & Shared Components

At runtime there are **two independent `ToolRegistry` instances** with different scopes:

| Registry | Scope | Created in | Purpose |
|---|---|---|---|
| `AppState.tool_registry` | Gateway-wide (shared) | `clawseed-gateway/src/lib.rs` | `/api/tools` endpoint visibility, global tool listing |
| `Agent.tool_registry` | Per-connection (isolated) | `clawseed-agent/src/agent.rs` (`Agent::builder().build()`) | Actual tool dispatch during agent turns |

Implications:
- `/api/tools` may show tools (from remote connections) that a given agent cannot actually invoke
- Remote tools must be registered in **both** registries to be both visible and executable
- In single-connection scenarios (current Android demo), the two registries are effectively in sync

**Shared components**: `AppState` holds `Arc<dyn Provider>`, `Arc<dyn Memory>`, `Arc<dyn Observer>`, `model: String`, and `temperature: f64`. Gateway connections use `from_config_with_shared_components()` to reuse these, avoiding per-connection provider (HTTP connection pools) and memory (SQLite connections) duplication. HookRunner and ToolRegistry remain per-connection (SecurityPolicy rate limits and remote tools must be isolated). Config updates via `/api/config` do **not** rebuild shared components — restart the gateway for provider/model/temperature/memory changes to take effect.

## MCP Status (planned, not yet implemented)

The `ToolSource::Mcp` enum variant and `McpConfig` schema exist, and `DefaultToolRegistry` supports per-server tool filtering. However, all MCP types in `crates/clawseed-agent/src/tools.rs` (`McpRegistry`, `DeferredMcpToolSet`, `McpToolWrapper`, `ToolSearchTool`) are **stubs** — they return empty collections or errors. There is no MCP protocol client library. The gateway has wiring that calls `McpRegistry::connect_all()`, but it returns immediately without connecting. Do not treat MCP as a usable capability.

## Runtime Init Chain

The initialization flow from entry point to running agent:

```
CLI (clawseed/src/main.rs)
  └→ Gateway: run_gateway() (clawseed-gateway/src/lib.rs)
       ├─ Creates AppState with shared provider, memory, observer, model, temperature, tool_registry
       └─ Each WebSocket connection (clawseed-gateway/src/ws.rs):
            ├─ Agent::from_config_with_shared_components() — reuses shared components
            │    ├─ Reuses state.provider, state.mem, state.observer, state.model, state.temperature
            │    ├─ Creates per-connection tools, hooks, dispatcher, skill index
            │    └─ Agent::builder().build() — creates agent-local tool_registry
            ├─ Remote tools: register to shared registry + inject into agent
            └─ Message loop: agent.chat() / agent.run()

Webhook (clawseed-gateway/src/handlers.rs)
  └→ Agent::from_config_with_shared_components() — same shared components, per-request Agent

Chat mode (clawseed/src/main.rs)
  └→ Agent::from_config() directly — creates own provider/memory, no gateway layer
```

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

## History Management

Each agent turn appends messages to a conversation history (`Vec<ChatMessage>`) that is sent to the LLM on every request. Unbounded history growth causes token overflow and cost escalation, so the agent applies automatic trimming:

- **`trim_history()`** — Drops the oldest non-system messages when history exceeds `max_history` (default 50), always preserving the system prompt at position 0
- **`truncate_tool_result()`** — Truncates oversized tool output to `max_chars`, keeping the head (2/3) and tail (1/3) with a `[... N characters truncated ...]` marker
- **`estimate_history_tokens()`** — Rough token count estimation (`content.len() / 4 + 4` per message) for budget decisions

```
System prompt (always kept)
  ↓
User message ─→ LLM response ─→ tool result ─→ ...
  ↑                                            │
  └──── trim_history() removes oldest ─────────┘
```

This ensures long-running sessions remain within token budgets without losing the system prompt.

## Memory System

History is the short-term conversation context sent to the LLM; Memory is the long-term knowledge store that persists across sessions. They serve different purposes:

| | History | Memory |
|---|---------|--------|
| **Scope** | Current session | Cross-session, persistent |
| **Storage** | In-memory `Vec<ChatMessage>` | SQLite database |
| **Lifecycle** | Cleared when session ends | Survives restarts |
| **Access** | Automatic (sent to LLM each turn) | Explicit (tools call `memory.recall()`) |
| **Content** | Full conversation text | Structured entries with metadata |

Memory is backed by `clawseed-memory`, implementing the `Memory` trait from `clawseed-api`:

```
┌─────────────────────────────────────┐
│            Memory trait              │
│  store / recall / get / list /      │
│  forget / count / health_check      │
└─────────────┬───────────────────────┘
              │
     ┌────────┴────────┐
     │                  │
┌────┴─────┐     ┌─────┴──────┐
│SqliteMemory│    │ NoneMemory │
│ (default)  │    │ (fallback) │
└────┬──────┘    └────────────┘
     │
┌────┴──────────────────────────────┐
│          Retrieval Engine          │
│  ┌──────────────┐ ┌─────────────┐ │
│  │   Vector     │ │    BM25     │ │
│  │  Similarity  │ │  Keyword    │ │
│  │  (embedding) │ │  Search     │ │
│  └──────┬───────┘ └──────┬──────┘ │
│         └────┬───────────┘        │
│              ↓                     │
│        Hybrid Ranking              │
└────────────────────────────────────┘
```

Key features:
- **Hybrid search**: Combines vector similarity (semantic) and BM25 (keyword) with configurable weights; controlled by `SearchMode` enum (`Hybrid` / `Embedding` / `Bm25`)
- **Memory categories**: `Core` (persistent knowledge), `Daily` (ephemeral), `Conversation` (context), `Custom(String)` (user-defined)
- **Namespace isolation**: `recall_namespaced()` filters by namespace for multi-tenant or per-user separation
- **Export**: `export()` with `ExportFilter` supports filtering by namespace, session, category, and time range
- **Graceful degradation**: If SQLite initialization fails, `NoneMemory` is used as a no-op fallback — tools that depend on memory simply skip the feature

## Design Principles

1. **Explicit over implicit** — `all_tools()` lists every tool; the full capability set is visible at a glance
2. **Declarative over imperative** — Config drives composition, not code changes
3. **Traits at boundaries** — Core depends on abstractions; implementations live outside
4. **Graceful degradation** — Missing capability → tool skips the feature; failed memory → NoneMemory fallback; flaky provider → ReliableProvider retries

## Crate Overview

| Crate | Role | Depends on api | Depends on agent |
|-------|------|:--------------:|:----------------:|
| `clawseed-api` | Trait definitions only | — | — |
| `clawseed-agent` | Agent loop, hooks, dispatch, parsing, runtime assembly | yes | — |
| `clawseed-tools` | 25+ built-in tools | yes | no |
| `clawseed-providers` | LLM provider implementations | yes | no |
| `clawseed-memory` | SQLite-backed memory + vector search | yes | no |
| `clawseed-config` | TOML config schema and loading | yes | no |
| `clawseed-gateway` | Axum HTTP/WS server + remote tool bridge | yes | yes |
| `clawseed` | Binary (CLI) | — | — |
