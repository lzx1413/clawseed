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

Run local interactive chat:
```bash
./target/release/clawseed chat                              # Default config
./target/release/clawseed chat --model gpt-4o               # Override model
./target/release/clawseed chat --temperature 0.5             # Override temperature
./target/release/clawseed chat --system-prompt "You are..."  # Override system prompt
```

Android demo app:
```bash
./tools/build-clawseed-android.sh aarch64 build             # Cross-compile gateway binary
cd clients/android && ./gradlew assembleDebug                # Build APK
adb install -r app/build/outputs/apk/debug/app-debug.apk    # Install
```

## Architecture

ClawSeed is a Rust AI agent runtime with trait-based plugin architecture. 8 workspace crates + 1 Android client with unidirectional dependency flow:

```
clawseed-api (traits only, no impls)
  ← clawseed-agent (orchestration + runtime assembly: loop, hooks, dispatch, security, parser, bootstrapping)
    ← clawseed-tools (25+ built-in tools)
    ← clawseed-providers (LLM backends: Anthropic, Gemini, OpenAI, Bedrock, DeepSeek, Ollama, etc.)
    ← clawseed-memory (SQLite + vector/keyword search)
      ← clawseed-gateway (Axum HTTP/WS server, remote tool bridge)
  ← clawseed-config (TOML config, loaded from ~/.clawseed/config.toml)
  ← clawseed (CLI binary)

clients/android (Kotlin/Compose demo app, runs gateway on-device)
```

> **Note:** The dependency arrows above show crate-level import direction. At runtime, `Agent::from_config_with_registry()` directly instantiates provider, memory, and tools from their respective crates — the agent crate is not a pure orchestration layer, it also owns runtime assembly.

### Core Traits (clawseed-api)

All extensibility flows through these traits — new capabilities register implementations without modifying agent code:

- **Tool**: `name()`, `description()`, `parameters_schema()`, `execute(args, ctx)` → `ToolResult`
- **ToolRegistry**: `register()`, `get_tool()`, `tool_specs()`, `unregister()` — unified tool management with `ToolSource` (BuiltIn/MCP/Remote). MCP tool source is defined in the enum and registry infrastructure supports it, but the actual MCP protocol client is not yet implemented — see "MCP Status" below.
- **Provider**: `chat()`, `stream_chat()`, `supports_native_tools()`, `warmup()`
- **Hook**: `before_tool_call()` → Continue/Cancel/Modify, `after_tool_call()`
- **Memory**: `store()`, `recall()`, `get()`, `forget()`, `list()`
- **ToolContext**: `workspace_dir()`, `get::<T>()` — type-safe capability lookup via `TypeId`

### Agent Assembly & Loop (clawseed-agent/src/agent.rs)

`Agent::from_config_with_registry()` is the primary constructor. It does runtime assembly — directly instantiates provider (via `ProviderFactoryRegistry`), memory (via `clawseed_memory::create_memory()`), and tools (via `clawseed_tools::registry::all_tools()`), then selects a dispatcher based on `provider.supports_native_tools()`. Tools depend on memory being constructed first; dispatcher depends on provider capabilities. All components are passed to `Agent::builder()` for final construction.

The agent loop then:
1. Accept message → add to history
2. Call provider with tool specs
3. Parse response via ToolDispatcher (XmlToolDispatcher for prompt-guided with multi-format fallback, NativeToolDispatcher for Anthropic/Gemini/OpenAI)
4. Dispatch tools (parallel when possible)
5. Feed results back to provider
6. Repeat until no tool calls or max iterations

### Capability Injection (clawseed-agent/src/context.rs)

Extensions inject typed capabilities (SecurityPolicy, Provider handles, etc.) via `Agent::builder().capability(Arc::new(...))`. Tools access them via `ctx.get::<T>()` — O(1) TypeId lookup, no string keys.

### Security (clawseed-agent/src/security/)

AutonomyLevel: ReadOnly / Supervised / Full. SecurityPolicy implements the `Hook` trait to globally intercept tool calls — validates commands, rate-limits actions, blocks forbidden paths (/etc/passwd, /etc/shadow, etc.). Always registered as the first hook in the pipeline.

### Remote Tools (clawseed-gateway)

Mobile clients connect via WebSocket, register tool specs, and execute tools locally. Gateway wraps them as `RemoteTool` implementing the `Tool` trait. Remote tool registration is a three-step flow:

1. **Register to shared registry** — `state.tool_registry.register_or_replace(tool, ToolSource::Remote { session })` so `/api/tools` reflects the tool globally
2. **Inject into per-connection Agent** — `agent.add_remote_tools(tools, session)` before processing each message, so the agent can actually invoke the tool
3. **Cleanup on disconnect** — `state.tool_registry.unregister_by_source(&ToolSource::Remote { session })`

This means the shared registry (`AppState.tool_registry`) and each agent's private registry (`Agent.tool_registry`) are separate instances — see "Dual Tool Registry" below. Agent code sees no difference between local and remote tools once injected.

### Tool Registration (clawseed-agent/src/tool_registry.rs)

`DefaultToolRegistry` implements the `ToolRegistry` trait using `DashMap` for lock-free concurrent access. Supports three tool sources (BuiltIn/MCP/Remote), glob-based filtering (`allowed_tools`/`denied_tools`), and per-MCP-server filtering. `all_tools()` in clawseed-tools creates enabled tools based on config.

### Dual Tool Registry

At runtime there are **two independent `ToolRegistry` instances** with different scopes:

| Registry | Scope | Created in | Purpose |
|---|---|---|---|
| `AppState.tool_registry` | Gateway-wide (shared) | `clawseed-gateway/src/lib.rs` | `/api/tools` endpoint visibility, global tool listing |
| `Agent.tool_registry` | Per-connection (isolated) | `clawseed-agent/src/agent.rs` (`Agent::builder().build()`) | Actual tool dispatch during agent turns |

Implications:
- `/api/tools` may show tools (from remote connections) that a given agent cannot actually invoke
- Remote tools must be registered in **both** registries to be both visible and executable (see "Remote Tools" above)
- In single-connection scenarios (current Android demo), the two registries are effectively in sync

### Provider Factory (clawseed-providers/src/factory.rs)

`ProviderFactoryRegistry` replaces the monolithic match chain. Each provider implements `ProviderFactory` trait with `name()`, `aliases()`, and `create()`. `Agent::from_config_with_registry()` accepts a custom registry for Android/embedded with minimal provider sets.

### Memory (clawseed-memory)

SQLite backend with hybrid search (BM25 keyword + vector embeddings). Categories: Core, Daily, Conversation, Custom. NoneMemory stub when disabled.

### MCP Status (planned, not yet implemented)

The `ToolSource::Mcp` enum variant and `McpConfig` schema exist, and `DefaultToolRegistry` supports per-server tool filtering. However, all MCP types in `crates/clawseed-agent/src/tools.rs` (`McpRegistry`, `DeferredMcpToolSet`, `McpToolWrapper`, `ToolSearchTool`) are **stubs** — they return empty collections or errors. There is no MCP protocol client library. The gateway has wiring that calls `McpRegistry::connect_all()`, but it returns immediately without connecting. Do not treat MCP as a usable capability.

### Runtime Init Chain

The initialization flow from entry point to running agent:

```
CLI (clawseed/src/main.rs)
  └→ Gateway: run_gateway() (clawseed-gateway/src/lib.rs)
       ├─ Creates AppState with shared tool_registry
       └─ Each WebSocket connection (clawseed-gateway/src/ws.rs):
            ├─ Agent::from_config() — creates per-connection Agent
            │    ├─ Instantiates provider, memory, tools
            │    └─ Agent::builder().build() — creates agent-local tool_registry
            ├─ Remote tools: register to shared registry + inject into agent
            └─ Message loop: agent.chat() / agent.run()

Chat mode (clawseed/src/main.rs)
  └→ Agent::from_config() directly — same assembly path, no gateway layer
```

### CETP (ClawSeed External Tool Protocol)

Protocol for third-party Android apps to expose read-only data tools to ClawSeed via ContentProvider. The Android client's `ExternalToolBridge` discovers Provider apps via PackageManager, calls `list_tools`/`execute_tool` via `ContentResolver.call()`, wraps them as `CetpProxyTool` (implementing `ClawSeedTool`), and registers them through the existing RemoteTool path — the gateway and agent see no difference. Providers self-manage authorization via `Binder.getCallingUid()` + `AUTH_REQUIRED` error codes. Dynamic refresh via `PACKAGE_ADDED`/`PACKAGE_REPLACED`/`PACKAGE_REMOVED` broadcasts. Protocol docs: `docs/zh/external-tool-protocol.md`, `docs/en/external-tool-protocol.md`. Provider tutorial: `docs/zh/cetp-provider-tutorial.md`, `docs/en/cetp-provider-tutorial.md`.

### Android Demo App (clients/android)

Full-featured chat client (Kotlin + Jetpack Compose) that runs the gateway on-device as a foreground service. Architecture: `MainActivity` → `ClawseedService` (manages gateway process + WebSocket) → `ChatViewModel`/`SessionsViewModel`/`SettingsViewModel` → Compose UI. The `lib/` module provides a reusable `ClawseedClient` WebSocket library. Features: streaming chat, Markdown rendering (tables, code blocks, inline formatting), extended thinking display, session management, on-device tools (device_info, get_location), CETP external tool bridge, LLM configuration with 11 provider presets, thinking mode toggle, debug mode.

## Key Conventions

- **Before every commit, run `./tools/ci-local.sh` to verify fmt/clippy/test pass.** This mirrors the CI pipeline. Fix all failures before committing.
- Rust edition 2024, minimum version 1.87
- Config loaded from `~/.clawseed/config.toml` with env var expansion
- Release profile uses fat LTO + codegen-units=1 + panic=abort
- Streaming-first: all providers support `stream_chat()` returning `BoxStream<StreamChunk>`
- Hook pipeline: before/after tool execution without core modifications
- Zero-cost defaults: disabled tools don't register; missing memory → NoneMemory fallback
