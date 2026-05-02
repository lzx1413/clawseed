# Android Demo Architecture

## Overview

The ClawSeed Android Demo is a complete on-device AI agent application. The entire agent stack runs on the Android device — the Rust-compiled Gateway binary runs as a foreground service process, while the Android client connects via WebSocket and registers device-side tools.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Android Device                        │
│                                                         │
│  ┌──────────────────────────────────────────────────┐  │
│  │  ClawseedService (Foreground Service)             │  │
│  │                                                   │  │
│  │  ┌────────────────────────────────────────────┐  │  │
│  │  │  libclawseed.so (Rust Gateway Process)     │  │  │
│  │  │  - Axum HTTP/WS server (port 42617)        │  │  │
│  │  │  - Agent loop + LLM calls                  │  │  │
│  │  │  - Built-in tool execution                 │  │  │
│  │  │  - Remote tool bridge                      │  │  │
│  │  └────────────────────────────────────────────┘  │  │
│  │         ↑ ProcessBuilder launch                   │  │
│  │         ↓ /health polling for readiness           │  │
│  └──────────────────────────────────────────────────┘  │
│                      ↕ WebSocket                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │  MainActivity (Compose UI)                        │  │
│  │  ┌────────────────────────────────────────────┐  │  │
│  │  │  ClawseedClient (SDK Library)              │  │  │
│  │  │  - OkHttp WebSocket connection             │  │  │
│  │  │  - Tool registration (device_info, etc.)   │  │  │
│  │  │  - Tool call handling                      │  │  │
│  │  │  - Streaming response callbacks            │  │  │
│  │  └────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────┘  │
│                                                         │
│                    ↕ Network                             │
│              LLM Provider (Anthropic, etc.)             │
└─────────────────────────────────────────────────────────┘
```

**Key design**: The entire agent stack runs on-device; LLM inference is called over the network to a cloud provider.

## Module Structure

```
clients/android/
├── lib/                          # ClawSeed SDK Library
│   ├── build.gradle.kts
│   └── src/main/kotlin/dev/clawseed/client/
│       ├── ClawseedClient.kt     # WebSocket client
│       └── ClawseedMessages.kt   # Message protocol types
├── app/                          # Demo Application
│   ├── build.gradle.kts
│   └── src/main/kotlin/dev/clawseed/demo/
│       ├── MainActivity.kt       # Compose UI
│       └── ClawseedService.kt    # Foreground service (Gateway process manager)
└── settings.gradle.kts
```

### lib — SDK Library

| Class | Responsibility |
|-------|---------------|
| `ClawseedClient` | WebSocket connection management, tool registration, message send/receive |
| `ToolSpec` | Tool specification (name, description, JSON Schema parameters) |
| `ToolCallRequest` | Tool invocation request from the server |
| `ToolCallResult` | Tool call result (Success / Failure) |
| `IncomingMessage` | Sealed class for all server-to-client message types |
| `ToolCallHandler` | Tool call handler interface (functional interface) |

### app — Demo Application

| Class | Responsibility |
|-------|---------------|
| `MainActivity` | Compose UI, connection/message/tool registration entry point |
| `ClawseedService` | Foreground service, manages Gateway process lifecycle |

## ClawseedClient — WebSocket Client

### Builder Pattern

```kotlin
val client = ClawseedClient.builder("ws://127.0.0.1:42617/ws/chat")
    .authToken("optional-token")
    .registerTool(ToolSpec(
        name = "device_info",
        description = "Get Android device information",
        parameters = """{"type":"object","properties":{},"required":[]}"""
    ))
    .toolCallHandler { request ->
        when (request.name) {
            "device_info" -> ToolCallResult.Success(queryDeviceInfo())
            else -> ToolCallResult.Failure("unknown tool")
        }
    }
    .onConnected { /* Connected */ }
    .onDisconnected { /* Disconnected */ }
    .onChunk { text -> /* Streaming text chunk */ }
    .onThinking { text -> /* Thinking process */ }
    .onDone { finalText -> /* Turn complete */ }
    .onToolCall { id, name, args -> /* Tool call notification */ }
    .onToolResult { id, name, output -> /* Tool result notification */ }
    .onAborted { /* Turn aborted */ }
    .onError { message -> /* Error */ }
    .build()

client.connect()           // Establish WebSocket connection
client.sendMessage("Hello") // Send user message
client.disconnect()         // Disconnect
```

### Connection Flow

1. OkHttp establishes WebSocket connection (readTimeout=0 for streaming)
2. `onOpen` callback automatically sends `register_tools` message
3. Server confirms tool registration (`tools_registered`)
4. Connection is ready for messages

### Tool Call Handling

```kotlin
// When a tool_call_request message is received
private fun dispatchToolCall(request: ToolCallRequest) {
    val handler = toolCallHandler ?: run {
        webSocket?.send(ToolCallResult.Failure("No handler").toJson(request.id).toString())
        return
    }
    executor.execute {
        val result = runCatching { handler.handleToolCall(request) }
            .getOrElse { ToolCallResult.Failure(it.message ?: "Exception") }
        webSocket?.send(result.toJson(request.id).toString())
    }
}
```

**Key points**:
- Single-threaded executor for tool call handling to avoid race conditions
- Exceptions are caught and wrapped as `ToolCallResult.Failure`
- Results are sent back to the server immediately via WebSocket

## Message Protocol

### Client → Server

| Type | Format | Description |
|------|--------|-------------|
| User message | `{"type":"message","content":"..."}` | Send a chat message |
| Tool registration | `{"type":"register_tools","tools":[...]}` | Register tool list |
| Tool result | `{"type":"tool_result","id":"...","output":"...","success":true}` | Return success result |
| Tool error | `{"type":"tool_error","id":"...","error":"...","success":false}` | Return execution error |

### Server → Client

| Type | Description |
|------|-------------|
| `session_start` | Session started (sessionId, name, resumed, messageCount) |
| `connected` | Connection confirmed |
| `chunk` | Streaming text chunk |
| `thinking` | Agent thinking process |
| `done` | Turn completed (full_response) |
| `tool_call` | Tool call notification (informational) |
| `tool_result` | Tool result notification (informational) |
| `tool_call_request` | Request client to execute a tool (requires response) |
| `tools_registered` | Tool registration confirmed (count, registered) |
| `result_acknowledged` | Result acknowledged |
| `chunk_reset` | Reset streaming output |
| `aborted` | Turn aborted |
| `error` | Error message |

### Complete Interaction Example

```
Client                                  Server
  │                                       │
  │ ──── WebSocket connect ────────────→  │
  │ ──── register_tools ──────────────→  │
  │ ←─── tools_registered ────────────  │
  │                                       │
  │ ──── message: "Tell me about device" │
  │ ←─── chunk: "Let me check" ────────  │
  │ ←─── tool_call_request ────────────  │
  │      {id:"tc1", name:"device_info"}  │
  │                                       │
  │ ──── tool_result ─────────────────→  │
  │      {id:"tc1", output:"..."}        │
  │                                       │
  │ ←─── chunk: "Your device is..." ───  │
  │ ←─── done ─────────────────────────  │
```

## ClawseedService — Foreground Service

### Lifecycle

```
onCreate()
  ├── Create notification channel
  └── startForeground("Starting clawseed gateway...")

onStartCommand()
  └── scope.launch { startGateway() }
        ├── Extract libclawseed.so binary
        ├── ensureConfig() — configuration initialization
        ├── ProcessBuilder launches Gateway process
        │     Env: HOME, XDG_CONFIG_HOME, XDG_DATA_HOME
        │     Args: gateway --port 42617
        │     API Key: loaded from .clawseed/api_key
        └── waitUntilReady()
              └── Poll http://127.0.0.1:42617/health
                    Every 500ms, max 40 attempts (20 seconds)

onDestroy()
  ├── Cancel coroutines
  ├── Destroy Gateway process
  └── Cleanup resources
```

### Binary Extraction and Execution

```kotlin
// useLegacyPackaging = true in build.gradle.kts
// libclawseed.so is extracted to nativeLibraryDir
val binary = File(applicationInfo.nativeLibraryDir, "libclawseed.so")

// Launch as subprocess
process = ProcessBuilder(binary.absolutePath, "gateway", "--port", "42617")
    .redirectErrorStream(true)
    .also { pb ->
        pb.environment()["HOME"] = filesDir.absolutePath
        pb.environment()["CLAWSEED_API_KEY"] = apiKey
    }
    .start()
```

**Why the `.so` naming**: Android APKs only allow `.so` files to be packaged in `jniLibs/`, but this is actually an executable Rust binary, executed via `ProcessBuilder` rather than `System.loadLibrary()`.

### Configuration Management

`ensureConfig()` handles initialization and patching:

1. Creates `~/.clawseed/` and `workspace/` directories
2. Generates initial config if `config.toml` doesn't exist
3. Patches missing fields if it does (workspace_dir, web feature enablement, etc.)
4. Auto-enables `web_fetch`, `http_request`, `web_search`
5. Adds `allowed_domains = ["*"]` for network tools

Initial config template:

```toml
workspace_dir = "{WORKSPACE_DIR}"

[gateway]

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "duckduckgo"
```

### Readiness Detection

```kotlin
private suspend fun waitUntilReady() {
    val healthUrl = "http://127.0.0.1:42617/health"
    repeat(MAX_HEALTH_ATTEMPTS) {  // 40 attempts
        val code = // HTTP GET healthUrl
        if (code in 200..299) {
            isReady = true
            readyCallbacks.forEach { it() }  // Notify MainActivity
            return
        }
        delay(500)  // 500ms interval
    }
    // Timeout → stop service
    stopSelf()
}
```

## Network Security Configuration

```xml
<network-security-config>
    <domain-config cleartextTrafficPermitted="true">
        <domain includeSubdomains="false">127.0.0.1</domain>
        <domain includeSubdomains="false">localhost</domain>
    </domain-config>
</network-security-config>
```

Only localhost cleartext connections are allowed (Gateway runs locally on port 42617).

## Permissions

| Permission | Purpose |
|------------|---------|
| `INTERNET` | Network access (LLM API, WebSocket) |
| `FOREGROUND_SERVICE` | Run foreground service |
| `FOREGROUND_SERVICE_SPECIAL_USE` | Android 14+ foreground service type declaration |
| `POST_NOTIFICATIONS` | Android 13+ notification permission |

## Build Configuration

| Item | Value |
|------|-------|
| minSdk | 26 (Android 8.0) |
| targetSdk / compileSdk | 36 (Android 15) |
| Java version | 17 |
| Compose BOM | 2026.04.01 |
| OkHttp | 4.12.0 |
| Kotlin Coroutines | 1.9.0 |
| useLegacyPackaging | true (binary extraction) |

## Steps to Customize the Demo

1. **Add tools**: Define new `ToolSpec` and corresponding handler logic in `MainActivity.kt`
2. **Modify UI**: Adjust Compose layout
3. **Add permissions**: Declare device capabilities (camera, location, etc.) in `AndroidManifest.xml`
4. **Configure Gateway**: Modify `ClawseedService.INITIAL_CONFIG` to adjust defaults
5. **API Key**: Place in `filesDir/.clawseed/api_key` file
