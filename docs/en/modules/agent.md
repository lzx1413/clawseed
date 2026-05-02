# clawseed-agent — Agent Core Runtime

## Overview

`clawseed-agent` is the core agent crate, responsible for tool dispatch, hook execution, security policy, cron scheduling, and more. It is the hub connecting Provider, Tool, Memory, and Hook.

## Core Structures

### Agent — Agent Registry

```rust
pub struct Agent {
    provider: Box<dyn Provider>,
    tool_registry: Arc<dyn ToolRegistry>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    tool_dispatcher: Box<dyn ToolDispatcher>,
    capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    workspace_dir: PathBuf,
    // ...
}
```

Agent is a **registry** that manages all tool sources (built-in, MCP, remote) through the `ToolRegistry` trait, and manages the Hook pipeline through `HookRunner`. Core code has no knowledge of specific tool implementations — extensions simply add entries to the registry.

### AgentBuilder — Builder Pattern

```rust
let agent = Agent::builder()
    .provider(provider)
    .tools(tools)                    // Option 1: pass tool list, auto-builds DefaultToolRegistry
    .tool_registry(registry)         // Option 2: pass pre-built ToolRegistry (takes priority)
    .memory(memory)
    .observer(observer)
    .tool_dispatcher(dispatcher)
    .workspace_dir(path)
    .capability(Arc::new(security_policy))
    .allowed_tools(Some(vec!["file_*".into()]))   // Glob pattern tool allowlist
    .denied_tools(Some(vec!["shell".into()]))      // Glob pattern tool denylist
    .mcp_tool_filters(Some(filters))               // Per-MCP-server filtering
    .hook_runner(Some(Arc::new(hook_runner)))       // Hook pipeline
    .build()?;

// Build from config (optionally with a custom ProviderFactoryRegistry)
let agent = Agent::from_config(&config).await?;
let agent = Agent::from_config_with_registry(&config, Some(provider_factory_registry)).await?;
```

## Module Architecture

### agent_loop.rs — Agent Loop Entry Point

Provides the gateway-compatible `turn()` call interface:

1. Receive user message
2. Build system prompt (calls `prompt.rs`)
3. Send to LLM
4. Parse response
5. If tool calls present → enter tool loop
6. Return final text response

### tool_loop.rs — Tool Loop

Manages the tool loop execution flow:

1. Parse tool calls from LLM response
2. Execute before_hook for each tool call
3. Execute tool
4. Execute after_hook
5. Format results, send back to LLM
6. Repeat until LLM returns text-only

### tool_execution.rs — Single Tool Execution

- Tools are looked up via `tool_registry.get_tool(name)` (returns `Arc<dyn Tool>`, O(1) hash lookup)
- Wraps tool execution with observer event recording, duration measurement, error handling, and cancellation support

### dispatcher.rs — Tool Dispatcher

Two implementations:

| Dispatcher | Use Case | How It Works |
|------------|----------|--------------|
| `NativeToolDispatcher` | Providers with native tool calling | Extracts `tool_calls` directly from response |
| `XmlToolDispatcher` | Providers without native tool calling | Tries ◁▷ format first, falls back to multi-format parser |

### parser.rs — Tool Call Parser

Multi-format tool call parsing supporting 12+ LLM output formats:

- OpenAI native JSON `tool_calls` array
- XML tags: `◁▷`, `<toolcall>`, `<tool-call>`, `<invoke>`
- MiniMax `<invoke>` format
- Markdown code blocks (` ```tool_call `)
- Anthropic `<FunctionCall>` tags
- GLM shortened format
- Perl/hash-ref style
- xAI grok ` ```tool <name> ` format

`XmlToolDispatcher::parse_response()` tries ◁▷ format first (deterministic prompt-guided parsing), then falls back to `parser::parse_tool_calls()` with the original response text for multi-format parsing.

**Security design**: Raw JSON without explicit wrappers is never extracted, preventing prompt injection attacks.

### context.rs — Tool Context

```rust
pub struct AgentToolContext {
    workspace_dir: PathBuf,
    capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl ToolContext for AgentToolContext {
    fn workspace_dir(&self) -> &Path { &self.workspace_dir }
    fn get<T: 'static>(&self) -> Option<&T> {
        self.capabilities.get(&TypeId::of::<T>())
            .and_then(|arc| arc.downcast_ref::<T>())
    }
}
```

### hooks.rs — Hook Runner

```rust
pub struct HookRunner {
    hooks: Vec<Box<dyn Hook>>,
}
```

Execution flow:
1. `run_before_tool_call()` — Iterates hooks sequentially
   - First hook to return `Cancel` stops the pipeline
   - `Modify` passes the modified call to the next hook
2. `fire_after_tool_call()` — Notifies all hooks, observation only, no modification

**Declarative hook chain**: Hooks can be declared in the `[hooks]` config section. `HookFactoryRegistry` creates Hook instances by `hook_type`. In `from_config()`, `SecurityPolicy` is always auto-registered as the first hook in the pipeline.

```rust
pub trait HookFactory: Send + Sync {
    fn hook_type(&self) -> &str;
    fn create(&self, config: &serde_json::Value) -> Option<Box<dyn Hook>>;
}
```

### tool_registry.rs — Tool Registry Implementation

`DefaultToolRegistry` is the default implementation of the `ToolRegistry` trait:

- Uses `DashMap` for lock-free concurrent access, safe in async contexts
- `ToolSpec` caching with write-time invalidation to avoid recomputation
- Three-layer glob pattern filtering: denied takes precedence → allowed allowlist → per-MCP-server filtering
- `register_all()` for bulk registration, `unregister_by_source()` for bulk removal by source

### security/ — Security Policy

- `mod.rs` — `SecurityPolicy` struct
  - Autonomy levels (ReadOnly / Supervised / Full)
  - Command allowlists
  - Medium-risk command list (touch, rm, cp, mv, mkdir, chmod, chown, kill)
  - Path restrictions (`/etc/passwd`, `/etc/shadow`, `/etc/ssh`, `/root/.ssh`)
  - Action rate limiting (`max_actions_per_hour`)
  - **Implements `Hook` trait**: `before_tool_call()` checks autonomy level, rate limits, command allowlists, and path guards; `after_tool_call()` records action count. SecurityPolicy is always registered as the first hook in the pipeline, and is no longer injected as a Capability
- `pairing.rs` — `PairingGuard` for device pairing verification with constant-time comparison
- `secrets.rs` — `SecretStore` credential management, `WebAuthnManager` support

### cron/ — Cron Scheduling

- `scheduler.rs` — Job execution engine, tracks jobs and run history
- `store.rs` — Persistent storage
  - `add_agent_job()` / `add_shell_job()` — Create cron jobs
  - `update_job()` / `remove_job()` — Modify/delete
  - `list_jobs()` / `due_jobs()` — Query
  - `record_run()` / `list_runs()` — Run history
  - `sync_declarative_jobs()` — Sync from config files
- `types.rs` — `CronJob`, `Schedule`, `SessionTarget`, `DeliveryConfig`
- `mod.rs` — Security integration
  - `validate_shell_command()` — Security policy check
  - `add_shell_job_with_approval()` — Approval before persistence

### prompt.rs — Modular System Prompt Builder

The system prompt is assembled from pluggable `PromptSection` implementations via `SystemPromptBuilder`:

```
SystemPromptBuilder::with_defaults()
  ├── DateTimeSection       — Current date and time
  ├── IdentitySection       — AIEOS identity + personality markdown files
  ├── WorkspaceSection      — Working directory path
  ├── ToolsSection          — Available tool descriptions
  ├── SafetySection         — Safety rules (autonomy-level-aware)
  └── ToolHonestySection    — Tool honesty constraints
```

Custom sections can be added via `SystemPromptBuilder::add_section()`.

### personality.rs — Personality File Loader

Loads well-known markdown files from the workspace directory:

| File | Purpose |
|------|---------|
| `SOUL.md` | Core personality and behavioral guidelines |
| `IDENTITY.md` | Name, role, background |
| `USER.md` | User preferences and context |
| `AGENTS.md` | Multi-agent coordination rules |
| `TOOLS.md` | Tool usage guidelines |
| `HEARTBEAT.md` | Periodic self-check instructions |
| `BOOTSTRAP.md` | First-run initialization instructions |
| `MEMORY.md` | Memory management guidelines |

Files are truncated at 20K characters. A default `SOUL.md` is auto-generated on first run.

### identity.rs — AIEOS Identity System

Supports AIEOS v1.1 (AI Entity Object Specification) — a structured JSON format for portable AI identity. Covers identity, psychology, linguistics, motivations, capabilities, physicality, history, and interests.

- `load_aieos_identity()` — loads from file or inline JSON
- `aieos_to_system_prompt()` — renders AIEOS identity to markdown
- Handles both official generator shape and simplified JSON formats via normalization

See [Personality & Identity Tutorial](../tutorials/personality-and-identity.md) for full documentation.

### Other Modules

| Module | Responsibility |
|--------|---------------|
| `cost.rs` | Token-based cost tracking |
| `observer.rs` | Event emission (NoopObserver by default, defined locally in clawseed-agent) |
| `observability.rs` | Re-exports Observer types for external consumers |
| `approval.rs` | Approval workflow for risky operations |
| `history.rs` | Conversation history management |
| `parser.rs` | Multi-format tool call parsing (12+ LLM output formats) |
| `health.rs` | Health check stubs |
