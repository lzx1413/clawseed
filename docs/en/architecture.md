# ClawSeed Architecture Overview

## Overview

ClawSeed is an AI agent runtime written in Rust. It connects to LLM providers (Anthropic, Gemini, Bedrock, OpenAI-compatible endpoints, and more), acts through pluggable tools, and serves clients over HTTP/WebSocket.

Core design principle: **traits at boundaries, implementations outside, core never changes**.

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
| `Provider` | LLM inference backend | Implement in `clawseed-providers` |
| `Tool` | Agent-callable capability | Implement in `clawseed-tools`, or register remote tools via WebSocket |
| `Hook` | Tool call interceptor | Implement `before_tool_call` / `after_tool_call` |
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

## Security Model

- **Autonomy levels**: `ReadOnly` / `Supervised` / `Full`
- **SecurityPolicy**: Injected as a capability; tools check via `ctx.get::<SecurityPolicy>()`
- **Command allowlists**: `allowed_commands` validates shell commands
- **Path guards**: Blocks access to sensitive paths (`/etc/passwd`, `/root/.ssh`, etc.)
- **Rate limiting**: `max_actions_per_hour` limits actions per session
- **Hook pipeline**: `Hook::before_tool_call()` can cancel or modify any tool call

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
