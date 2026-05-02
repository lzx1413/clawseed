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
| `XmlToolDispatcher` | Providers without native tool calling | Parses ◁▷-wrapped JSON from text |

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

### Other Modules

| Module | Responsibility |
|--------|---------------|
| `cost.rs` | Token-based cost tracking |
| `observer.rs` | Event emission (no-op by default) |
| `approval.rs` | Approval workflow for risky operations |
| `history.rs` | Conversation history management |
| `prompt.rs` | System prompt construction |
| `health.rs` | Health check stubs |
