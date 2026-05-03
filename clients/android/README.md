# ClawSeed Android Demo

[中文版](README_zh.md)

An Android client for ClawSeed that runs the clawseed gateway natively on-device (compiled as `.so`), communicating via WebSocket and REST to provide LLM chat, tool calling, session management, and configuration editing.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  UI Layer (Jetpack Compose + Material 3)            │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │ChatScreen│  │ Drawer   │  │SettingsScreen    │  │
│  │  + Bubble│  │(Sessions)│  │(Form / TOML edit)│  │
│  └────┬─────┘  └────┬─────┘  └────────┬─────────┘  │
│       │              │                 │             │
│  ┌────┴─────┐  ┌─────┴────────┐  ┌────┴──────────┐ │
│  │ChatVM    │  │SessionsVM    │  │SettingsVM     │ │
│  └────┬─────┘  └─────┬────────┘  └────┬──────────┘ │
├───────┼──────────────┼─────────────────┼────────────┤
│  Data Layer           │                             │
│  ┌────┴───────────────┴────────────────┴──────────┐ │
│  │           ClawseedService (Foreground)         │ │
│  │  - Launches & manages gateway process          │ │
│  │  - WebSocket connection & messaging            │ │
│  │  - Tool registration & dispatch                │ │
│  └────────────────────┬───────────────────────────┘ │
│  ┌────────────────────┤                             │
│  │GatewayApi (REST)   │  LocalStore (DataStore)     │
│  └────────────────────┘                             │
└───────────────────────┼─────────────────────────────┘
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
| `lib` | `dev.clawseed.client` | Reusable WebSocket client library |

### Directory Structure

```
app/src/main/kotlin/dev/clawseed/demo/
├── MainActivity.kt              # Entry Activity, binds to Service
├── ClawseedApp.kt               # Root Composable, navigation + drawer
├── ClawseedService.kt           # Foreground service, gateway process & WebSocket
├── CoordinateConverter.kt       # WGS84 → GCJ-02 coordinate conversion
├── data/
│   ├── ChatModels.kt            # Data models (ChatEntry, ChatSession, ToolInfo, etc.)
│   ├── GatewayApi.kt            # REST API client (OkHttp)
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
        └── SettingsViewModel.kt # LLM provider config, model fetching

lib/src/main/kotlin/dev/clawseed/client/
├── ClawseedClient.kt            # WebSocket client (Builder pattern)
└── ClawseedMessages.kt          # Message type definitions & JSON serialization
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
- **Gateway built-in tools**: web_fetch, http_request, web_search, etc.

### LLM Configuration
- 11 provider presets (DeepSeek, Qwen, Moonshot, GLM, Doubao, Baidu, OpenAI, Anthropic, OpenRouter, Ollama, Custom)
- Model list fetching (direct or via gateway proxy)
- Thinking Mode toggle
- Form editing or raw TOML editing

### Markdown Rendering
- Headings (h1-h6), code blocks (with language label + copy button), lists, tables
- Inline formatting: **bold**, *italic*, `monospace`

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
- Gson (JSON)
- AndroidX DataStore (local persistence)
- `libclawseed.so` (clawseed gateway native binary, JNI legacy packaging)
