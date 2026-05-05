# ClawSeed Android Demo

[中文版](README_zh.md)

An Android client for ClawSeed that runs the clawseed gateway natively on-device (compiled as `.so`), communicating via WebSocket and REST to provide LLM chat, tool calling, session management, and configuration editing.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│  UI Layer (Jetpack Compose + Material 3)                 │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────────┐  │
│  │ChatScreen│  │ Drawer   │  │SettingsScreen         │  │
│  │  + Bubble│  │(Sessions)│  │(Form / TOML edit)     │  │
│  └────┬─────┘  └────┬─────┘  └──────────┬────────────┘  │
│       │              │                    │               │
│  ┌────┴─────┐  ┌─────┴────────┐  ┌──────┴────────────┐  │
│  │ChatVM    │  │SessionsVM    │  │SettingsVM         │  │
│  └────┬─────┘  └─────┬────────┘  └──────┬────────────┘  │
├───────┼──────────────┼──────────────────┼────────────────┤
│  SDK Layer                                                  │
│  ┌────┴────────────────┴─────────────────┴──────────────┐ │
│  │  sdk:android (ClawSeedAndroid, SessionManager,       │ │
│  │              ChatAccumulator, ClawSeedViewModel)      │ │
│  ├───────────────────────────────────────────────────────┤ │
│  │  sdk:embedded (EmbeddedGateway, GatewayService,      │ │
│  │                 GatewayConfigManager)                 │ │
│  ├───────────────────────────────────────────────────────┤ │
│  │  sdk:core (ClawSeedSession, ChatClient, GatewayClient│ │
│  │            ToolRegistry, models)                      │ │
│  └───────────────────────────┬───────────────────────────┘ │
│  ┌────────────────────┐  ┌──┴──────────────────┐          │
│  │LocalStore (DataStore)│  │GatewayApi (REST)   │          │
│  └────────────────────┘  └─────────────────────┘          │
└───────────────────────────┼───────────────────────────────┘
                            │ localhost:42617
                    ┌───────┴───────┐
                    │ clawseed      │
                    │ gateway       │
                    │ (native .so)  │
                    └───────────────┘
```

### Modules

| Module | Package | Description |
|--------|---------|-------------|
| `app` | `dev.clawseed.demo` | Main application: UI, Service, ViewModels, data layer |
| `sdk:core` | `dev.clawseed.sdk.core` | Core abstractions: session, chat/WebSocket client, tool registry, models |
| `sdk:android` | `dev.clawseed.sdk.android` | Android-specific: ClawSeedAndroid singleton, SessionManager, ChatAccumulator, CETP external tool bridge |
| `sdk:embedded` | `dev.clawseed.sdk.embedded` | Embedded gateway: process management, config, foreground service |

### Directory Structure

```
app/src/main/kotlin/dev/clawseed/demo/
├── MainActivity.kt              # Entry Activity, binds to Service
├── ClawseedApp.kt               # Root Composable, navigation + drawer
├── CoordinateConverter.kt       # WGS84 → GCJ-02 coordinate conversion
├── data/
│   ├── ChatModels.kt            # Data models (ChatEntry, ChatSession, ToolInfo, etc.)
│   └── LocalStore.kt            # DataStore local persistence
└── ui/
    ├── navigation/
    │   └── ClawseedNavHost.kt   # Navigation routes (Chat / Settings)
    ├── chat/
    │   ├── ChatScreen.kt        # Chat main screen
    │   ├── ChatViewModel.kt     # Chat logic + tool registration
    │   └── components/
    │       ├── ChatBottomBar.kt  # Input field + send/stop buttons
    │       ├── MessageBubble.kt  # Message bubbles (user/assistant/tool/thinking)
    │       └── MarkdownContent.kt# Markdown renderer (headings/code/tables/inline)
    ├── drawer/
    │   ├── SessionDrawer.kt     # Session history sidebar
    │   └── SessionsViewModel.kt # Session CRUD operations
    └── settings/
        ├── SettingsScreen.kt    # Configuration UI (form + TOML modes)
        └── SettingsViewModel.kt # LLM provider config, search engine, model fetching

sdk/core/src/main/kotlin/dev/clawseed/sdk/core/
├── ClawSeed.kt                  # Session factory interface
├── ClawSeedConfig.kt            # SDK configuration
├── ClawSeedSession.kt           # Session interface
├── DefaultClawSeedSession.kt    # Default session implementation
├── client/
│   ├── ChatClient.kt            # WebSocket chat client (connect, send, tool dispatch)
│   ├── GatewayClient.kt         # REST API client (sessions, config, tools, status)
│   └── ReconnectPolicy.kt       # Auto-reconnect policy
├── model/
│   ├── ChatEvent.kt             # Chat event types (chunk, thinking, tool_call, etc.)
│   ├── ConnectionState.kt       # WebSocket connection states
│   ├── Gateway.kt               # Gateway status model
│   └── Session.kt               # Session model
└── tool/
    ├── ClawSeedTool.kt          # Tool interface
    ├── ToolRegistry.kt          # Client-side tool registry
    ├── ToolResult.kt            # Tool execution result
    └── ToolSpec.kt              # Tool specification

sdk/android/src/main/kotlin/dev/clawseed/sdk/android/
├── ClawSeedAndroid.kt           # Singleton: SDK initialization + gateway client access
├── SessionManager.kt            # Session lifecycle management
├── ChatAccumulator.kt           # Accumulates streaming chunks into messages
├── AccumulatedMessage.kt        # Accumulated message model
├── ClawSeedViewModel.kt         # ViewModel base for chat
└── cetp/
    ├── CetpConstants.kt         # CETP v1 protocol constants
    ├── CetpModels.kt            # Data classes (DiscoveredProvider, AuthRequiredEvent, etc.)
    ├── CetpClient.kt            # ContentResolver.call() wrapper
    ├── ExternalToolBridge.kt    # Discovers providers, bridges tools into ToolRegistry
    └── PackageChangeReceiver.kt # BroadcastReceiver for package install/update/uninstall

sdk/embedded/src/main/kotlin/dev/clawseed/sdk/embedded/
├── EmbeddedGateway.kt           # Gateway process lifecycle management
├── EmbeddedGatewayConfig.kt     # Gateway startup config (port, binary name, timeouts)
├── GatewayConfigManager.kt      # TOML config creation, patching, web_search defaults
├── GatewayService.kt            # Android foreground service for gateway process
└── GatewayState.kt              # Gateway state model
```

## Features

### Chat
- Real-time streaming via WebSocket
- Full message history per session
- Extended Thinking display (collapsible)
- Debug mode showing full prompt and token estimate

### Session Management
- Create / resume / rename / delete sessions
- Auto-naming from first user message
- Session history in navigation drawer

### Tool Calling
- **On-device tools** (registered via WebSocket):
  - `device_info` — device model, manufacturer, Android version
  - `get_location` — GPS location (WGS84→GCJ-02) + reverse geocoding
- **CETP external tools** (discovered from third-party apps):
  - Automatically discovers apps implementing the [CETP v1 protocol](../../docs/en/external-tool-protocol.md)
  - Provider tools are namespaced (e.g., `finance__get_portfolio_holdings`) and registered as RemoteTools
  - Supports `AUTH_REQUIRED` flow with resolution hints and authorize intents
  - Dynamic refresh on package install/update/uninstall
- **Gateway built-in tools**: web_fetch, http_request, web_search, etc.

### LLM Configuration
- 11 provider presets (DeepSeek, Qwen, Moonshot, GLM, Doubao, Baidu, OpenAI, Anthropic, OpenRouter, Ollama, Custom)
- Model list fetching (direct or via gateway proxy)
- Thinking Mode toggle
- Form editing or raw TOML editing

### Search Engine Configuration
- Search engine selector (Bing / Tavily)
- Tavily API Key input with password visibility toggle
- Link to get free Tavily API key (1,000 calls/month)
- Provider and key written to `[web_search]` TOML section

### Gateway Status & Tools
- Status card showing current Provider, Model, Memory backend
- Expandable registered tools list with source type (Built-in / Remote / MCP)

### Markdown Rendering
- Headings (h1-h6), code blocks (with language label + copy button), lists, tables
- Inline formatting: **bold**, *italic*, `monospace`

### Developer Options
- Debug Query Message toggle — shows full LLM prompt and token estimate per message

## Default Gateway Configuration

The app auto-generates a TOML config on first launch with web features enabled:

```toml
workspace_dir = "{WORKSPACE_DIR}"

[gateway]
session_persistence = true

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "bing"
```

To use Tavily instead of Bing, change the `[web_search]` section via Settings UI or TOML:

```toml
[web_search]
enabled = true
provider = "tavily"
tavily_api_key = "tvly-..."
```

## Communication Protocol

### WebSocket

URL: `ws://127.0.0.1:42617/ws/chat?session_id={id}`

| Direction | Type | Description |
|-----------|------|-------------|
| → | `message` | Send user message |
| → | `register_tools` | Register on-device tools |
| → | `tool_result` | Return tool execution result |
| ← | `chunk` | Streaming text fragment |
| ← | `thinking` | Thinking process fragment |
| ← | `tool_call_request` | Request tool execution |
| ← | `done` | Response complete |
| ← | `aborted` | Response aborted |

### REST API

Base URL: `http://127.0.0.1:42617`

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/sessions` | List sessions |
| GET | `/api/sessions/{id}/messages` | Session message history |
| PUT | `/api/sessions/{id}` | Rename session |
| DELETE | `/api/sessions/{id}` | Delete session |
| POST | `/api/sessions/{id}/abort` | Abort generation |
| GET | `/api/config` | Get TOML configuration |
| PUT | `/api/config` | Update configuration |
| GET | `/api/tools` | List registered tools |
| GET | `/api/status` | Gateway status |
| GET | `/api/provider/models` | Fetch model list via gateway proxy |

## Building

```bash
# Build the clawseed native binary first (requires NDK)
cd /path/to/claw-seed
./tools/build-clawseed-android.sh aarch64 build

# Build APK
cd clients/android
./gradlew assembleDebug

# Install
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

### Dependencies

- Android SDK 36, minSdk 26
- Kotlin + Jetpack Compose (Material 3)
- OkHttp 4.12 (HTTP + WebSocket)
- kotlinx-serialization-json (JSON)
- AndroidX DataStore (local persistence)
- `libclawseed.so` (clawseed gateway native binary, JNI legacy packaging)
