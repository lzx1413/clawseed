# ClawSeed Android SDK Usage Guide

> Status: Active | Date: 2026-05-05

## 1. Choose the Right Module

| Use case | Dependency |
|----------|------------|
| JVM or Android app connecting to an existing gateway | `:sdk:core` |
| Android app that wants lifecycle helpers and accumulators | `:sdk:android` |
| Android app running the gateway on-device | `:sdk:embedded` |

## 2. Core Usage

Create a session:

```kotlin
val session = ClawSeed.createSession(
    ClawSeedConfig(
        gatewayUrl = "http://127.0.0.1:3000",
        authToken = null,
    )
)
```

Connect and send a message:

```kotlin
viewModelScope.launch {
    session.connect()
    session.sendMessage("Hello from ClawSeed")
}
```

Collect events:

```kotlin
viewModelScope.launch {
    session.events.collect { event ->
        when (event) {
            is ChatEvent.TextChunk -> appendToUi(event.content)
            is ChatEvent.Done -> markTurnComplete(event.fullResponse)
            is ChatEvent.Error -> showError(event.message)
            else -> Unit
        }
    }
}
```

## 3. Register Client Tools

Register a simple lambda tool:

```kotlin
session.tools.register(
    name = "device_info",
    description = "Get device information",
    parameters = """{"type":"object","properties":{}}""",
) { _ ->
    ToolResult.Success("ok")
}
```

Register a structured tool:

```kotlin
class DeviceInfoTool : ClawSeedTool {
    override val name = "device_info"
    override val description = "Get device information"
    override val parametersSchema = buildJsonObject {
        put("type", "object")
    }

    override suspend fun execute(args: JsonObject): ToolResult {
        return ToolResult.Success("ok")
    }
}

session.tools.register(DeviceInfoTool())
```

## 4. Android Initialization

Initialize from `Application.onCreate()`:

```kotlin
class DemoApp : Application() {
    override fun onCreate() {
        super.onCreate()
        ClawSeedAndroid.init(
            context = this,
            config = ClawSeedConfig(gatewayUrl = "http://10.0.2.2:3000")
        )
    }
}
```

Use the shared session manager:

```kotlin
viewModelScope.launch {
    val session = ClawSeedAndroid.sessionManager().connect()
    session.sendMessage("Hello")
}
```

## 5. Use ChatAccumulator

`ChatAccumulator` is the easiest way to bridge raw chat events into UI state:

```kotlin
val accumulator = ChatAccumulator(session)
accumulator.startIn(viewModelScope)

viewModelScope.launch {
    accumulator.messages.collect { messages ->
        render(messages)
    }
}
```

Use `addUserMessage(content)` when your UI should show a sent user message immediately before the gateway finishes echoing turn state.

## 6. Embedded Gateway Usage

Start the gateway locally:

```kotlin
val gateway = EmbeddedGateway(context)

viewModelScope.launch {
    gateway.start()
    ClawSeedAndroid.init(context, gateway.localConfig())
}
```

Important prerequisites:

- Package `libclawseed.so` into the app's `jniLibs`.
- Enable legacy jni lib extraction if the process must execute the binary from `nativeLibraryDir`.
- Declare the required foreground service permissions if using `GatewayService`.

## 7. HTTP Administrative Operations

Use `GatewayClient` for non-chat operations:

```kotlin
val client = ClawSeedAndroid.gatewayClient()

viewModelScope.launch {
    val sessions = client.sessions().getOrElse { emptyList() }
    val status = client.status().getOrNull()
}
```

Common operations:

- `sessions()` to list chats
- `sessionMessages(id)` to load history
- `renameSession(id, name)` to rename a session
- `deleteSession(id)` to delete a session
- `config()` and `updateConfig(toml)` to manage gateway config
- `models()` or `fetchProviderModels()` to discover models

## 8. Recommended Integration Pattern

For most Android apps, the practical integration path is:

1. Initialize `ClawSeedAndroid` once in the application layer.
2. Use `SessionManager` to own the active session.
3. Use `ChatAccumulator` to feed UI state.
4. Register client tools after obtaining the session.
5. Use `GatewayClient` for session management and settings pages.

## 9. Documentation Map

- `sdk-design.md`: architecture and design rationale
- `sdk-implementation.md`: current code structure and module mapping
- `sdk-usage.md`: integration guide and examples