# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
cargo build                          # Debug build
cargo build --release                # Optimized (LTO, stripped)
cargo check                          # Fast type-check
cargo clippy                         # Lint
cargo test                           # All tests
cargo test -p clawseed-agent         # Single crate
cargo test --test agent_integration  # Single test file
cargo fmt                            # Format check
```

Run the gateway:
```bash
./target/release/clawseed gateway --host 0.0.0.0 --port 3000
```

## Architecture

ClawSeed is a Rust AI agent runtime with trait-based plugin architecture. 10 workspace crates with unidirectional dependency flow:

```
clawseed-api (traits only, no impls)
  ← clawseed-agent (orchestration: loop, hooks, dispatch, security)
    ← clawseed-tools (25+ built-in tools)
    ← clawseed-providers (LLM backends: Anthropic, Gemini, OpenAI, Bedrock, Ollama, etc.)
    ← clawseed-memory (SQLite + vector/keyword search)
      ← clawseed-gateway (Axum HTTP/WS server, remote tool bridge)
  ← clawseed-config (TOML config, loaded from ~/.clawseed/config.toml)
  ← clawseed-parser (tool call extraction from LLM output)
  ← clawseed (CLI binary)
```

### Core Traits (clawseed-api)

All extensibility flows through these traits — new capabilities register implementations without modifying agent code:

- **Tool**: `name()`, `description()`, `parameters_schema()`, `execute(args, ctx)` → `ToolResult`
- **ToolRegistry**: `register()`, `get_tool()`, `tool_specs()`, `unregister()` — unified tool management with `ToolSource` (BuiltIn/MCP/Remote)
- **Provider**: `chat()`, `stream_chat()`, `supports_native_tools()`, `warmup()`
- **Hook**: `before_tool_call()` → Continue/Cancel/Modify, `after_tool_call()`
- **Memory**: `store()`, `recall()`, `get()`, `forget()`, `list()`
- **ToolContext**: `workspace_dir()`, `get::<T>()` — type-safe capability lookup via `TypeId`

### Agent Loop (clawseed-agent/src/agent.rs)

1. Accept message → add to history
2. Call provider with tool specs
3. Parse response via ToolDispatcher (XmlToolDispatcher for prompt-guided, NativeToolDispatcher for Anthropic/Gemini/OpenAI)
4. Dispatch tools (parallel when possible)
5. Feed results back to provider
6. Repeat until no tool calls or max iterations

### Capability Injection (clawseed-agent/src/context.rs)

Extensions inject typed capabilities (SecurityPolicy, Provider handles, etc.) via `Agent::builder().capability(Arc::new(...))`. Tools access them via `ctx.get::<T>()` — O(1) TypeId lookup, no string keys.

### Security (clawseed-agent/src/security/)

AutonomyLevel: ReadOnly / Supervised / Full. SecurityPolicy implements the `Hook` trait to globally intercept tool calls — validates commands, rate-limits actions, blocks forbidden paths (/etc/passwd, /etc/shadow, etc.). Always registered as the first hook in the pipeline.

### Remote Tools (clawseed-gateway)

Mobile clients connect via WebSocket, register tool specs, and execute tools locally. Gateway wraps them as `RemoteTool` implementing the `Tool` trait, registered via `tool_registry.register_or_replace(tool, ToolSource::Remote { session })`. Agent sees no difference between local and remote tools.

### Tool Registration (clawseed-agent/src/tool_registry.rs)

`DefaultToolRegistry` implements the `ToolRegistry` trait using `DashMap` for lock-free concurrent access. Supports three tool sources (BuiltIn/MCP/Remote), glob-based filtering (`allowed_tools`/`denied_tools`), and per-MCP-server filtering. `all_tools()` in clawseed-tools creates enabled tools based on config.

### Provider Factory (clawseed-providers/src/factory.rs)

`ProviderFactoryRegistry` replaces the monolithic match chain. Each provider implements `ProviderFactory` trait with `name()`, `aliases()`, and `create()`. `Agent::from_config_with_registry()` accepts a custom registry for Android/embedded with minimal provider sets.

### Memory (clawseed-memory)

SQLite backend with hybrid search (BM25 keyword + vector embeddings). Categories: Core, Daily, Conversation, Custom. NoneMemory stub when disabled.

## Key Conventions

- Rust edition 2024, minimum version 1.87
- Config loaded from `~/.clawseed/config.toml` with env var expansion
- Release profile uses fat LTO + codegen-units=1 + panic=abort
- Streaming-first: all providers support `stream_chat()` returning `BoxStream<StreamChunk>`
- Hook pipeline: before/after tool execution without core modifications
- Zero-cost defaults: disabled tools don't register; missing memory → NoneMemory fallback
