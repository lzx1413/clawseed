<p align="center">
  <img src="assets/clawseed.png" alt="ClawSeed" width="240=0" />
</p>

<p align="center">
  <strong>A Rust AI agent runtime with remote tool execution.</strong>
</p>

<p align="center">
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache%202.0-blue.svg" alt="License" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-edition%202024-orange?logo=rust" alt="Rust Edition 2024" /></a>
  <a href="https://github.com/lzx1413/clawseed/actions/workflows/ci.yml"><img src="https://github.com/lzx1413/clawseed/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <strong>English</strong> | <a href="README_zh.md">中文</a>
</p>

---

ClawSeed is an AI agent **runtime** written in Rust. It connects to LLM providers (Anthropic, Gemini, Bedrock, DeepSeek, OpenAI-compatible, and more), acts through pluggable tools, and serves clients over HTTP/WebSocket. It ships with an Android demo app that runs the full agent stack on-device.

An agent runtime should do three things: receive messages, call an LLM, execute tools. Everything else — channels, dashboards, integrations — belongs to the application layer. ClawSeed provides crates with stable traits; applications compose them.

```toml
# A Discord bot application
[dependencies]
clawseed-agent = "0.7"
clawseed-providers = "0.7"
serenity = "0.12"          # App chooses its own SDK

# An Android application
[dependencies]
clawseed-gateway = "0.7"
clawseed-agent = "0.7"

# A CLI tool
[dependencies]
clawseed-agent = "0.7"
clawseed-tools = "0.7"
```

The agent runs server-side, but mobile clients (Android, iOS) can register their own tools over WebSocket. When the agent calls one of these tools, the gateway forwards the request to the client for execution. This lets the agent access device capabilities — contacts, camera, sensors — without device-specific code on the server.

ClawSeed borrows its trait-based architecture from [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw), with a smaller scope and a different positioning: ZeroClaw bundled channels, dashboards, hardware, and SOP into one binary (an application); ClawSeed provides crates for applications to assemble (a runtime). ClawSeed also adds an Android demo app, extended thinking support, and a modular prompt builder that ZeroClaw does not have.

## Architecture

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

Dependency flow is one-way: **api ← agent ← tools / providers / memory ← gateway**. Nothing points back up.

## Remote tool calls

Mobile clients register tool specs when they connect over WebSocket. The gateway wraps each spec as a `RemoteTool` — a `Tool` trait implementation that bridges execution to the client:

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

Flow:

1. Client connects and sends `register_tools` with tool specs (name, description, JSON Schema)
2. Gateway creates a `RemoteTool` for each spec, adds to agent's tool list
3. Agent calls the tool; `RemoteTool::execute()` sends `tool_call_request` to client over WebSocket
4. Client executes locally, responds with `tool_result` or `tool_error`
5. Gateway correlates response by call ID (30s timeout), returns result to agent

The agent loop has no branching for remote vs. local tools — both implement the `Tool` trait. Remote tools do not use `ToolContext` (no access to server-side memory, security policy, or other capabilities).

### Android SDK

```kotlin
val client = ClawseedClient(
    gatewayUrl = "ws://localhost:3000/ws/chat",
    tools = listOf(
        ToolSpec("local_contacts", "Query phone contacts", contactsSchema),
        ToolSpec("camera", "Take a photo", cameraSchema),
    )
) { request ->
    when (request.name) {
        "local_contacts" -> ToolCallResult.Success(queryContacts(request.args))
        "camera" -> ToolCallResult.Success(takePhoto(request.args))
        else -> ToolCallResult.Failure("unknown tool")
    }
}
client.connect()
```

The SDK also runs the gateway binary on-device as a foreground service — the entire agent stack runs on the Android device, with the LLM provider accessed over the network.

### Android Demo App

A full-featured Android chat client is included at [`clients/android/`](clients/android/). It runs the clawseed gateway natively (compiled as `.so`), providing:

- Real-time streaming chat with Markdown rendering (headings, code blocks, tables, bold/italic)
- Extended thinking display — collapsible cards showing the model's chain-of-thought
- Session management (create, resume, rename, delete, auto-naming)
- On-device tools: `device_info`, `get_location` (WGS84 to GCJ-02 with reverse geocoding)
- LLM configuration UI with 11 provider presets (DeepSeek, Qwen, OpenAI, Anthropic, Ollama, etc.)
- Thinking mode toggle for models that support extended thinking (e.g. DeepSeek V4)
- Debug mode showing full LLM prompt and token estimates

See [`clients/android/README.md`](clients/android/README.md) for architecture details.

## Crates

| Crate | Role | Depends on api | Depends on agent |
|-------|------|:-:|:-:|
| `clawseed-api` | Trait definitions only | — | — |
| `clawseed-agent` | Agent loop, hooks, dispatch, parsing | yes | — |
| `clawseed-tools` | 25+ built-in tools | yes | no |
| `clawseed-providers` | LLM provider implementations | yes | no |
| `clawseed-memory` | SQLite-backed memory + vector search | yes | no |
| `clawseed-config` | TOML config schema and loading | yes | no |
| `clawseed-gateway` | Axum HTTP/WS server + remote tool bridge | yes | yes |
| `clawseed` | Binary (CLI) | — | — |

## Quick start

```bash
git clone https://github.com/lzx1413/clawseed.git
cd clawseed
cargo build --release

# Run the gateway (HTTP/WebSocket server for mobile clients)
./target/release/clawseed gateway --host 0.0.0.0 --port 3000

# Or start a local interactive chat session (no server needed)
./target/release/clawseed chat
./target/release/clawseed chat --model gpt-4o --temperature 0.5

# Build and install the Android demo app (requires NDK)
./tools/build-clawseed-android.sh aarch64 build
cd clients/android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Both modes read `~/.clawseed/config.toml`. Minimal config:

```toml
[providers.models.default]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"

[agent]
workspace_dir = "/home/user/workspace"
```

## Extending ClawSeed

### Add a tool

Implement the `Tool` trait in `clawseed-tools`, register in `all_tools()`:

```rust
pub struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Does something useful" }
    fn parameters_schema(&self) -> Value { /* JSON Schema */ }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        if let Some(policy) = ctx.get::<SecurityPolicy>() {
            policy.can_act()?;
        }
        // ...
    }
}
```

### Add a hook

Implement the `Hook` trait to intercept tool calls:

```rust
pub struct AuditHook;

impl Hook for AuditHook {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult {
        log::info!("tool {} called", call.name);
        HookResult::Continue
    }

    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult {
        log::info!("tool {} → {:?}", result.name, result.status);
        HookResult::Continue
    }
}
```

`HookResult` has three variants: `Continue`, `Cancel(String)`, `Modify(ToolCall)`.

### Add a provider

Implement the `Provider` trait in `clawseed-providers`, add to the factory. Supports native tool calling, streaming, vision, and prompt caching.

### Add a capability

Inject any `Send + Sync + 'static` type into the agent — tools discover it at runtime:

```rust
// At construction (gateway)
agent_builder.capability(Arc::new(my_custom_service));

// At execution (tool)
if let Some(svc) = ctx.get::<MyCustomService>() {
    svc.do_thing();
}
```

## Built-in tools

**File operations** — read, write, edit, glob search, content search
**Web** — HTTP request, web fetch, web search (DuckDuckGo)
**Memory** — store, recall, forget, purge, export
**Automation** — cron add / list / remove / run / update
**Development** — shell, git operations, PDF read
**Utilities** — calculator, LLM sub-task, knowledge base, model routing, backup

Tools that the agent doesn't need are excluded by `allowed_tools` in config — they don't register, don't consume tokens.

## Security

- **Autonomy levels** — `ReadOnly` / `Supervised` / `Full`, configured per deployment
- **SecurityPolicy** — implements the `Hook` trait to globally intercept tool calls before execution
- **Command allowlists** — `allowed_commands` in SecurityPolicy validates shell commands
- **Path guards** — `forbidden_path_argument()` blocks sensitive paths (`/etc/passwd`, `/root/.ssh`, etc.)
- **Rate limiting** — `max_actions_per_hour` limits total actions per session
- **Hook pipeline** — `Hook::before_tool_call()` can cancel or modify any tool call before execution

## Design principles

1. **Explicit over implicit** — `all_tools()` lists every tool; the full capability set is visible at a glance
2. **Declarative over imperative** — config drives composition, not code changes
3. **Traits at boundaries** — core depends on abstractions; implementations live outside
4. **Graceful degradation** — missing capability → tool skips the feature; failed memory → NoneMemory fallback; flaky provider → ReliableProvider retries

## Acknowledgments

ClawSeed's trait-based architecture and provider/tool/memory abstraction patterns are derived from [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw).

The key difference is positioning: ZeroClaw is an application (channels, dashboards, hardware, and SOP bundled into one binary); ClawSeed is a runtime (crates that applications assemble). This means:

- No bundled channels — applications integrate their own messaging SDKs
- No bundled dashboard — applications build their own UI (e.g. the Android demo app)
- Added native remote tool calls for mobile clients
- Added unified `Hook` trait and `TypeId`-based capability injection
- Added `ProviderFactory` registry for platform-specific provider sets (Android/embedded)
- Added extended thinking support with reasoning content round-trip for tool calls
- Added Android demo app running the full agent stack on-device

## License

Dual-licensed: [MIT](LICENSE-MIT) OR [Apache 2.0](LICENSE-APACHE). You may choose either.
