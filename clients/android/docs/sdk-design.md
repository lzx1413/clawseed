# ClawSeed Android SDK Design Document

> Status: Active | Date: 2026-05-05

## 1. Overview

### 1.1 Background

The current Android client (`clients/android/`) is a demo app with a thin `lib/` module. The `lib/` module contains `ClawseedClient`（callback-based WebSocket client）and `ClawseedMessages`（message parsing with `org.json`）. The `app/` module contains gateway process management, REST API client, session management, and all UI code tightly coupled together.

### 1.2 Goal

Transform the Android client into a first-class, publishable SDK with stable API interfaces, enabling other Android and JVM applications to integrate ClawSeed as a dependency.

### 1.3 Design Principles

- **Stable Public API**: Minimize breaking changes once published; use interfaces over concrete classes at API boundaries
- **Kotlin-first**: Coroutines/Flow natively, no callback-based API
- **Platform-agnostic core**: Core module runs on any JVM, not just Android
- **Layered architecture**: Each module adds one concern; consumers pick the layer they need
- **Zero-cost defaults**: Consumers only pay for what they use (e.g., embedded gateway is opt-in)

---

## 2. Module Architecture

```
sdk/core/        (dev.clawseed.sdk.core)      Pure Kotlin/JVM: protocol, REST, models, tools
  ^
  |
sdk/android/     (dev.clawseed.sdk.android)    Android lifecycle, ViewModel, ChatAccumulator
  ^
  |
sdk/embedded/    (dev.clawseed.sdk.embedded)   Embedded gateway process management (on-device binary)
```

### 2.1 Dependency Graph

```
                    +------------------+
                    |   clawseed-core  |   kotlin("jvm")
                    |  Pure Kotlin/JVM |   No Android imports
                    +--------+---------+
                             |
                    +--------v---------+
                    | clawseed-android |   com.android.library
                    | Lifecycle, VM,   |   Depends on core (api)
                    | ChatAccumulator  |
                    +--------+---------+
                             |
                    +--------v---------+
                    | clawseed-embedded|   com.android.library
                    | Gateway process  |   Depends on android (api)
                    | management       |
                    +------------------+
                             |
                    +--------v---------+
                    |    demo app      |   com.android.application
                    +------------------+
```

### 2.2 Maven Coordinates

| Module | Group | Artifact | Plugin |
|--------|-------|----------|--------|
| core | `dev.clawseed` | `clawseed-core` | `kotlin("jvm")` |
| android | `dev.clawseed` | `clawseed-android` | `com.android.library` |
| embedded | `dev.clawseed` | `clawseed-embedded` | `com.android.library` |

### 2.3 Consumer Dependency Scenarios

| Scenario | Dependency |
|----------|------------|
| JVM/server app connecting to remote gateway | `clawseed-core` |
| Android app connecting to remote gateway | `clawseed-android` |
| Android app with on-device gateway | `clawseed-embedded` |

---

## 3. `sdk/core` — Protocol, REST, Models, Tools

### 3.1 Package Structure

```
dev.clawseed.sdk.core/
  ClawSeed.kt                  Entry point object
  ClawSeedConfig.kt            Configuration data class
  ClawSeedSession.kt           Session interface + default implementation

dev.clawseed.sdk.core.model/
  ChatEvent.kt                 Sealed class for all streaming events
  Session.kt                   Session data models
  Gateway.kt                   Gateway status/config models
  ConnectionState.kt           Connection state enum

dev.clawseed.sdk.core.client/
  ChatClient.kt                WebSocket client (internal)
  GatewayClient.kt             REST API client (public)
  ReconnectPolicy.kt           Reconnect strategies

dev.clawseed.sdk.core.tool/
  ClawSeedTool.kt              Tool interface
  ToolResult.kt                Tool execution result
  ToolSpec.kt                  Tool specification
  ToolRegistry.kt              Client-side tool registry
```

### 3.2 Dependencies

```kotlin
// sdk/core/build.gradle.kts
plugins {
    kotlin("jvm")
    kotlin("plugin.serialization")
    `maven-publish`
}
dependencies {
    api("com.squareup.okhttp3:okhttp:4.12.0")
    api("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
}
```

**Why `kotlinx-serialization` instead of `org.json`/`Gson`:**
- `org.json.JSONObject` is Android-only, breaking pure JVM compatibility
- `Gson` is reflection-based and less type-safe
- `kotlinx-serialization` provides compile-time safety and works on all Kotlin targets

### 3.3 Public API: Entry Point

```kotlin
// ClawSeed.kt
object ClawSeed {
    /**
     * Create a new session. The session is not connected until [ClawSeedSession.connect] is called.
     */
    fun createSession(config: ClawSeedConfig): ClawSeedSession
}
```

```kotlin
// ClawSeedConfig.kt
data class ClawSeedConfig(
    /** Gateway base URL, e.g. "http://192.168.1.100:3000" */
    val gatewayUrl: String,
    /**
     * Bearer token provider for authentication.
     * Called on each request, supporting dynamic token updates (e.g., after pairing).
     * For a static token, use the convenience constructor.
     */
    val authTokenProvider: () -> String? = { null },
    /** Automatic reconnection policy on unexpected disconnects. */
    val reconnectPolicy: ReconnectPolicy = ReconnectPolicy.ExponentialBackoff(),
) {
    /** Convenience constructor for static auth token. */
    constructor(
        gatewayUrl: String,
        authToken: String?,
        reconnectPolicy: ReconnectPolicy = ReconnectPolicy.ExponentialBackoff(),
    ) : this(gatewayUrl, { authToken }, reconnectPolicy)
}
```

### 3.4 Public API: Session

```kotlin
// ClawSeedSession.kt
interface ClawSeedSession : Closeable {
    /** Current connection state. */
    val connectionState: StateFlow<ConnectionState>

    /** Session info, available after session_start event. */
    val sessionInfo: StateFlow<SessionInfo?>

    /** Stream of all chat events from the gateway. */
    val events: SharedFlow<ChatEvent>

    /** Client-side tool registry. Register tools before connecting. */
    val tools: ToolRegistry

    /** REST API client for the gateway. */
    val gateway: GatewayClient

    /** Connect to the gateway. Optionally resume an existing session. */
    suspend fun connect(sessionId: String? = null)

    /** Disconnect gracefully. */
    suspend fun disconnect()

    /**
     * Send a chat message to the agent.
     *
     * **Threading contract:**
     * - This method enqueues the message on the WebSocket and returns immediately.
     * - It is safe to call from any thread (internally synchronized).
     *
     * **State contract:**
     * - CONNECTED: message is sent immediately.
     * - RECONNECTING: message is queued and sent when the connection is restored.
     * - DISCONNECTED / CONNECTING: throws [IllegalStateException].
     *
     * **Concurrency contract:**
     * - If the gateway is still processing a previous turn, the server will respond
     *   with a `ChatEvent.Error(code = "SESSION_BUSY")` via [events]. The SDK does
     *   NOT queue or retry — the caller decides whether to wait for `Done`/`Aborted`
     *   before sending again.
     *
     * @throws IllegalStateException if the session is not connected and not reconnecting.
     */
    fun sendMessage(content: String, debug: Boolean = false)

    /** Abort the current agent turn. */
    suspend fun abort()
}
```

### 3.5 Data Models: ChatEvent

Sealed class hierarchy mapping 1:1 to the gateway WebSocket protocol messages:

```kotlin
// ChatEvent.kt
sealed class ChatEvent {
    /** Session established or resumed. */
    data class SessionStarted(
        val sessionId: String,
        val name: String?,
        val resumed: Boolean,
        val messageCount: Int,
        val version: Int?,
    ) : ChatEvent()

    /** WebSocket connection acknowledged. */
    data class Connected(val message: String, val version: Int?) : ChatEvent()

    /** Incremental text delta from the assistant. */
    data class TextChunk(val content: String) : ChatEvent()

    /** Incremental thinking/reasoning delta. */
    data class ThinkingChunk(val content: String) : ChatEvent()

    /** Signals the start of a new message (flush accumulated content). */
    data object ChunkReset : ChatEvent()

    /** Agent turn complete. Contains the full assembled response. */
    data class Done(val fullResponse: String) : ChatEvent()

    /** Server-side tool call started (informational). */
    data class ToolCallStarted(
        val id: String,
        val name: String,
        val args: JsonObject,
    ) : ChatEvent()

    /** Server-side tool call completed (informational). */
    data class ToolCallCompleted(
        val id: String,
        val name: String,
        val output: String,
    ) : ChatEvent()

    /** Remote tool call request — SDK dispatches this to ToolRegistry automatically. */
    data class ToolCallRequested(
        val id: String,
        val name: String,
        val args: JsonObject,
    ) : ChatEvent()

    /** Confirmation that tools were registered on the server. */
    data class ToolsRegistered(val count: Int, val registered: Int) : ChatEvent()

    /** Server acknowledged a tool result we sent. */
    data class ResultAcknowledged(val id: String) : ChatEvent()

    /** Agent turn was aborted. */
    data object Aborted : ChatEvent()

    /** Session title was auto-generated or updated. */
    data class TitleUpdated(val title: String) : ChatEvent()

    /** Error from the gateway. */
    data class Error(val message: String, val code: String? = null) : ChatEvent()

    /** Debug prompt info (when debug=true). */
    data class DebugPrompt(val messages: String, val estimatedTokens: Int) : ChatEvent()
}
```

### 3.6 Data Models: Session & Gateway

```kotlin
// Session.kt
data class SessionInfo(
    val sessionId: String,
    val name: String?,
    val resumed: Boolean,
    val messageCount: Int,
)

@Serializable
data class SessionSummary(
    @SerialName("session_id") val id: String,
    val name: String? = null,
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("last_activity") val lastActivity: String = "",
    @SerialName("message_count") val messageCount: Int = 0,
) {
    /** Created-at as epoch milliseconds for UI convenience. */
    val createdAtMillis: Long get() = parseIsoToEpochMillis(createdAt)
    /** Last-activity as epoch milliseconds for UI convenience. */
    val lastActivityMillis: Long get() = parseIsoToEpochMillis(lastActivity)
}

@Serializable
data class SessionMessage(
    val role: String,
    val content: String? = null,
    @SerialName("tool_name") val toolName: String? = null,
    @SerialName("tool_args") val toolArgs: String? = null,
    @SerialName("tool_result") val toolResult: String? = null,
    val success: Boolean? = null,
)
```

```kotlin
// Gateway.kt
@Serializable
data class GatewayStatus(
    val provider: String? = null,
    val model: String = "",
    val temperature: Double = 0.7,
    @SerialName("memory_backend") val memoryBackend: String? = null,
    val paired: Boolean = false,
    @SerialName("gateway_port") val gatewayPort: Int = 0,
)

@Serializable
data class ToolInfo(
    val name: String = "",
    val description: String = "",
    @SerialName("source_type") val sourceType: String = "builtin",
    val source: String? = null,
)

// ConnectionState.kt
enum class ConnectionState {
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
    RECONNECTING,
}
```

### 3.7 Tool System

#### Interface (for complex tools with state)

```kotlin
// ClawSeedTool.kt
interface ClawSeedTool {
    val name: String
    val description: String
    val parametersSchema: JsonObject
    suspend fun execute(args: JsonObject): ToolResult
}
```

```kotlin
// ToolResult.kt
sealed class ToolResult {
    data class Success(val output: String) : ToolResult()
    data class Failure(val error: String) : ToolResult()
}
```

```kotlin
// ToolSpec.kt
data class ToolSpec(
    val name: String,
    val description: String,
    val parameters: JsonObject,
)
```

#### Registry (supports both interface and lambda registration)

```kotlin
// ToolRegistry.kt
class ToolRegistry {
    /** Register a tool implementation. */
    fun register(tool: ClawSeedTool)

    /** Register a simple tool via lambda. */
    fun register(
        name: String,
        description: String,
        parameters: String,
        handler: suspend (JsonObject) -> ToolResult,
    )

    /** Unregister a tool by name. */
    fun unregister(name: String): Boolean

    /** List all registered tool specs. */
    fun registeredTools(): List<ToolSpec>
}
```

#### Tool Registration Example

```kotlin
// Interface approach (for complex tools)
class DeviceInfoTool(private val context: Context) : ClawSeedTool {
    override val name = "device_info"
    override val description = "Get Android device information"
    override val parametersSchema = buildJsonObject {
        put("type", "object")
        putJsonObject("properties") {}
    }

    override suspend fun execute(args: JsonObject): ToolResult {
        val info = buildJsonObject {
            put("model", Build.MODEL)
            put("manufacturer", Build.MANUFACTURER)
        }
        return ToolResult.Success(info.toString())
    }
}

session.tools.register(DeviceInfoTool(context))
```

```kotlin
// Lambda approach (for simple tools)
session.tools.register(
    name = "get_battery",
    description = "Get current battery level",
    parameters = """{"type":"object","properties":{}}""",
) { args ->
    val level = batteryManager.getIntProperty(BATTERY_PROPERTY_CAPACITY)
    ToolResult.Success("""{"level": $level}""")
}
```

### 3.8 REST Client

```kotlin
// GatewayClient.kt
class GatewayClient(
    val baseUrl: String,
    val authTokenProvider: () -> String? = { null },
    httpClient: OkHttpClient = defaultHttpClient(),
) {
    /** Convenience constructor for static token. */
    constructor(baseUrl: String, authToken: String?, httpClient: OkHttpClient = defaultHttpClient())
        : this(baseUrl, { authToken }, httpClient)

    suspend fun health(): Result<HealthInfo>
    suspend fun status(): Result<GatewayStatus>
    suspend fun sessions(): Result<List<SessionSummary>>
    suspend fun sessionMessages(sessionId: String): Result<List<SessionMessage>>
    suspend fun renameSession(sessionId: String, name: String): Result<Unit>
    suspend fun deleteSession(sessionId: String): Result<Unit>
    suspend fun abortSession(sessionId: String): Result<Unit>
    suspend fun config(): Result<String>
    suspend fun updateConfig(toml: String): Result<Unit>
    suspend fun tools(): Result<List<ToolInfo>>
    suspend fun models(): Result<List<String>>

    /**
     * Fetch models directly from an external LLM provider's /models endpoint.
     * Used when the API key is available client-side and a direct connection
     * is preferred over the gateway proxy.
     */
    suspend fun fetchProviderModels(
        providerBaseUrl: String,
        apiKey: String,
    ): Result<List<String>>
}
```

**Design note — dynamic auth token:** The primary constructor accepts `authTokenProvider: () -> String?`
instead of a fixed `authToken: String?`. This supports scenarios where the token changes at runtime
(e.g., after pairing). The token lambda is evaluated on each HTTP request via an OkHttp Interceptor.
A convenience constructor accepting a static `String?` is provided for the common case.

### 3.9 Reconnection

Automatic reconnection on unexpected disconnects with configurable policy:

```kotlin
sealed class ReconnectPolicy {
    /** No automatic reconnection. */
    data object None : ReconnectPolicy()

    /** Exponential backoff with jitter. */
    data class ExponentialBackoff(
        val initialDelayMs: Long = 1000,
        val maxDelayMs: Long = 30_000,
        val maxAttempts: Int = Int.MAX_VALUE,
    ) : ReconnectPolicy()
}
```

**State machine:**

```
DISCONNECTED ──connect()──> CONNECTING ──success──> CONNECTED
      ^                         |                       |
      |                      failure                  failure/close
      |                         |                       |
      |                         v                       v
      +───────────────── RECONNECTING <─────────────────+
                              |
                         policy exhausted
                              |
                              v
                         DISCONNECTED (terminal)
```

- Jitter: random 0-50% added to computed delay to prevent thundering herd
- Session ID preserved across reconnects (gateway resumes existing session)
- User-initiated `disconnect()` skips reconnection

---

## 4. `sdk/android` — Lifecycle Integration

### 4.1 Package Structure

```
dev.clawseed.sdk.android/
  ClawSeedAndroid.kt            Android-aware entry point
  SessionManager.kt             Session lifecycle manager
  ChatAccumulator.kt            Streaming event accumulator (streaming text, thinking, message history)
  AccumulatedMessage.kt         Accumulated message sealed class
```

**Note on Compose:** This module intentionally has no Compose dependency. Compose utilities
like `rememberClawSeedSession()` are trivial wrappers (< 10 lines) that consumers can write
in their own app module. This avoids forcing the Compose compiler plugin and runtime onto
consumers who use the View system.

### 4.2 Dependencies

```kotlin
// sdk/android/build.gradle.kts
plugins {
    id("com.android.library")
    kotlin("android")
    `maven-publish`
}
dependencies {
    api(project(":sdk:core"))
    implementation("androidx.lifecycle:lifecycle-viewmodel-ktx:2.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
}
```

### 4.3 Android Entry Point

```kotlin
// ClawSeedAndroid.kt
object ClawSeedAndroid {
    /**
     * Initialize the SDK. Call once from Application.onCreate().
     * Sets up shared OkHttpClient and default config.
     */
    fun init(context: Context, config: ClawSeedConfig)

    /** Returns whether init() has completed successfully. */
    val isInitialized: Boolean

    /** Get the singleton session manager. */
    fun sessionManager(): SessionManager

    /** Get a GatewayClient configured with the init config. */
    fun gatewayClient(): GatewayClient

    /** Suspend until init() has completed. */
    suspend fun awaitInit()
}
```

### 4.4 Session Manager

The `SessionManager` is an Application-scoped singleton. It owns the `ClawSeedSession` instance,
which survives Activity recreation (rotation, configuration changes, navigation). This matches the
current architecture where `ClawseedService` (a foreground service) holds the WebSocket connection
independently of any Activity.

**Lifecycle ownership model — three distinct layers:**

| Layer | Owner | Lifetime | Destroyed when |
|-------|-------|----------|----------------|
| Gateway process | `EmbeddedGateway` / `GatewayService` | App process | App killed or `stop()` called |
| WebSocket session | `SessionManager` (Application singleton) | App foreground lifetime | Explicit `disconnect()` or app backgrounded (configurable) |
| UI observation | ViewModel / Composable | Activity/Fragment | Configuration change, navigation |

Activity destruction (rotation, config change) only tears down UI observation, never the session.

```kotlin
// SessionManager.kt
class SessionManager internal constructor(private val config: ClawSeedConfig) {
    /** Currently active session. Survives Activity recreation. */
    val activeSession: StateFlow<ClawSeedSession?>

    /**
     * Connect to a session, creating one if sessionId is null.
     * This is an IO-bound suspend function — call from a coroutine scope.
     */
    suspend fun connect(sessionId: String? = null): ClawSeedSession

    /** Disconnect the current session. */
    suspend fun disconnect()

    /**
     * Observe an Android lifecycle to manage the session connection.
     *
     * Behavior by lifecycle event:
     * - ON_STOP: no action (session stays connected for quick resume)
     * - ON_DESTROY: unbinds the observer only — does NOT disconnect the session.
     *
     * To disconnect the session when the app goes to background, use
     * [bindToProcessLifecycle] with `disconnectOnBackground = true` instead.
     *
     * This method is idempotent — calling it multiple times with different
     * lifecycles (e.g., after Activity recreation) is safe.
     */
    fun observeLifecycle(lifecycle: Lifecycle)

    /**
     * Bind to [ProcessLifecycleOwner] for app-level foreground/background transitions.
     *
     * @param disconnectOnBackground if true, disconnects the session when the app
     *        moves to background (ON_STOP). Reconnects on ON_START. Default: false.
     */
    fun bindToProcessLifecycle(disconnectOnBackground: Boolean = false)
}
```

### 4.5 Chat Accumulator

The gateway emits raw streaming events (`TextChunk`, `ThinkingChunk`, `ChunkReset`, `Done`, etc.).
Most apps need accumulated state: the current streaming text, thinking text, and a message history list.
`ChatAccumulator` bridges this gap so every consumer doesn't have to reimplement the accumulation logic
that currently lives in `ClawseedService`.

```kotlin
// ChatAccumulator.kt
package dev.clawseed.sdk.android

/**
 * Accumulates raw [ChatEvent]s from a [ClawSeedSession] into UI-friendly state flows.
 *
 * Replaces the manual accumulation that was previously done in ClawseedService
 * (streamingContent, thinkingContent, messages StateFlows).
 */
class ChatAccumulator(private val session: ClawSeedSession) {

    /** Accumulated streaming text for the current assistant turn. Reset on ChunkReset/Done. */
    val streamingContent: StateFlow<String>

    /** Accumulated thinking text for the current turn. Reset on ChunkReset/Done. */
    val thinkingContent: StateFlow<String>

    /** Ordered list of completed chat messages (user, assistant, tool call/result, thinking, etc.). */
    val messages: StateFlow<List<AccumulatedMessage>>

    /** Current session title (updated via TitleUpdated events). */
    val sessionTitle: StateFlow<String?>

    /**
     * Start collecting events from the session. Call once after connecting.
     * The accumulator observes [session.events] and updates its state flows.
     * Cancellation: tied to the provided [CoroutineScope].
     */
    fun startIn(scope: CoroutineScope)

    /** Clear all accumulated state (for session switch). */
    fun reset()
}

/** A completed chat message in the accumulated history. */
sealed class AccumulatedMessage {
    abstract val id: String
    abstract val timestamp: Long

    data class User(override val id: String, override val timestamp: Long, val content: String) : AccumulatedMessage()
    data class Assistant(override val id: String, override val timestamp: Long, val content: String) : AccumulatedMessage()
    data class ToolCall(override val id: String, override val timestamp: Long, val callId: String, val name: String, val args: String) : AccumulatedMessage()
    data class ToolResult(override val id: String, override val timestamp: Long, val callId: String, val name: String, val output: String) : AccumulatedMessage()
    data class Thinking(override val id: String, override val timestamp: Long, val content: String) : AccumulatedMessage()
    data class Error(override val id: String, override val timestamp: Long, val message: String) : AccumulatedMessage()
    data class Debug(override val id: String, override val timestamp: Long, val messagesJson: String, val estimatedTokens: Int) : AccumulatedMessage()
}
```

**Accumulation rules** (matching current `ClawseedService` behavior):
- `TextChunk` → append to `streamingContent`
- `ThinkingChunk` → append to `thinkingContent`
- `ChunkReset` → flush thinking to `messages` as `Thinking`, flush streaming to `messages` as `Assistant`, reset both
- `Done` → same flush as `ChunkReset`
- `ToolCallStarted` → append `ToolCall` to `messages`
- `ToolCallCompleted` → append `ToolResult` to `messages`
- `Error` → append `Error` to `messages`
- `Aborted` → reset streaming/thinking, append system message
- `TitleUpdated` → update `sessionTitle`
- `DebugPrompt` → append `Debug` to `messages`
- User messages are added by the app when calling `sendMessage()` (the accumulator provides an `addUserMessage(content)` method)

### 4.6 ViewModel Support

```kotlin
// ClawSeedViewModel.kt
open class ClawSeedViewModel(application: Application) : AndroidViewModel(application) {
    protected val session: ClawSeedSession by lazy {
        ClawSeedAndroid.sessionManager().let { /* obtain or create session */ }
    }
    /** Pre-wired accumulator bound to [viewModelScope]. */
    protected val accumulator: ChatAccumulator by lazy {
        ChatAccumulator(session).also { it.startIn(viewModelScope) }
    }
    val connectionState: StateFlow<ConnectionState>

    fun sendMessage(content: String)
    fun abort()
}
```

### 4.7 Note: Compose Integration

This module does **not** provide Compose utilities. Consumers using Jetpack Compose can
write a thin wrapper in their own app module:

```kotlin
// In your app module (which already has the Compose compiler plugin)
@Composable
fun rememberClawSeedSession(sessionId: String? = null): ClawSeedSession {
    val sessionManager = remember { ClawSeedAndroid.sessionManager() }
    val session = remember { mutableStateOf<ClawSeedSession?>(null) }
    LaunchedEffect(sessionId) {
        session.value = sessionManager.connect(sessionId)
    }
    DisposableEffect(Unit) {
        onDispose { /* session survives — managed by SessionManager */ }
    }
    return session.value ?: error("Session not yet connected")
}
```

This keeps the SDK free of Compose compiler/runtime dependencies, avoiding cost for
View-system consumers.

---

## 5. `sdk/embedded` — Gateway Process Management

### 5.1 Package Structure

```
dev.clawseed.sdk.embedded/
  EmbeddedGateway.kt            Gateway process lifecycle
  EmbeddedGatewayConfig.kt      Configuration
  GatewayState.kt               State sealed class
  GatewayConfigManager.kt       TOML config management
  GatewayService.kt             Reusable foreground service
```

### 5.2 Dependencies

```kotlin
// sdk/embedded/build.gradle.kts
plugins {
    id("com.android.library")
    kotlin("android")
    `maven-publish`
}
dependencies {
    api(project(":sdk:android"))
    implementation("androidx.core:core-ktx:1.16.0")
}
```

### 5.3 Native Binary Packaging & ABI Policy

The `sdk/embedded` module provides **process management only** — it does NOT bundle the
`libclawseed.so` binary. The binary is the consumer app's responsibility, just as a
WebView-based SDK doesn't ship the browser engine.

**Current build flow** (preserved for the demo app and documented for SDK consumers):

1. Cross-compile via `tools/build-clawseed-android.sh <arch> build`
2. Output goes to `app/src/main/jniLibs/{abi}/libclawseed.so`
3. Gradle packages it into the APK

**Consumer requirements:**

| Requirement | Why | Where to configure |
|-------------|-----|--------------------|
| Place `libclawseed.so` in `src/main/jniLibs/{abi}/` | Android extracts jniLibs to exec-allowed `nativeLibraryDir` | App's source tree |
| Set `jniLibs.useLegacyPackaging = true` | Without this, AGP leaves .so compressed inside APK; `ProcessBuilder` cannot exec from a zip | App's `build.gradle.kts` `android.packaging` block |
| Declare `<uses-permission android:name="android.permission.FOREGROUND_SERVICE"/>` | `GatewayService` runs as foreground service | App's `AndroidManifest.xml` |
| Declare `<uses-permission android:name="android.permission.FOREGROUND_SERVICE_SPECIAL_USE"/>` (API 34+) | Required on Android 14+ for `specialUse` foreground service type | App's `AndroidManifest.xml` |

**Supported ABIs:**

| ABI | Rust target | Status |
|-----|-------------|--------|
| `arm64-v8a` | `aarch64-linux-android` | Primary (most devices) |
| `x86_64` | `x86_64-linux-android` | Emulator / ChromeOS |
| `armeabi-v7a` | `armv7-linux-androideabi` | Legacy 32-bit |

**Binary discovery at runtime:**

`EmbeddedGateway` locates the binary via `context.applicationInfo.nativeLibraryDir` + `config.binaryName`
(default `"libclawseed.so"`). This is the same mechanism used by the current `ClawseedService.kt:242`.

**Future consideration:** A Gradle plugin (`clawseed-gradle-plugin`) could automate the cross-compilation
and jniLibs placement, but this is out of scope for the initial SDK release.

### 5.4 Embedded Gateway

```kotlin
// EmbeddedGateway.kt
class EmbeddedGateway(
    private val context: Context,
    private val config: EmbeddedGatewayConfig = EmbeddedGatewayConfig(),
) {
    /** Current gateway process state. */
    val state: StateFlow<GatewayState>

    /** Start the gateway process. Suspends until health check passes. */
    suspend fun start()

    /** Stop the gateway process gracefully. */
    suspend fun stop()

    /** Restart the gateway (stop + start). */
    suspend fun restart()

    /** Get a ClawSeedConfig pointing to this local gateway. */
    fun localConfig(): ClawSeedConfig
}
```

```kotlin
// EmbeddedGatewayConfig.kt
data class EmbeddedGatewayConfig(
    /** Port to run the gateway on. */
    val port: Int = 42617,
    /** Name of the native binary in jniLibs. */
    val binaryName: String = "libclawseed.so",
    /** Max time to wait for health check to pass. */
    val healthCheckTimeoutMs: Long = 20_000,
    /** Interval between health check polls. */
    val healthCheckIntervalMs: Long = 500,
)
```

```kotlin
// GatewayState.kt
sealed class GatewayState {
    data object Stopped : GatewayState()
    data object Starting : GatewayState()
    data class Running(val port: Int) : GatewayState()
    data class Failed(val error: String) : GatewayState()
}
```

### 5.5 Config Manager

Extracted from current `ClawseedService.ensureConfig()`:

```kotlin
// GatewayConfigManager.kt
class GatewayConfigManager(private val context: Context) {
    /** Ensure config file exists with required sections. Returns the config file. */
    fun ensureConfig(): File

    /** Get the .clawseed config directory. */
    fun configDir(): File

    /** Get the workspace directory. */
    fun workspaceDir(): File
}
```

### 5.6 Reusable Foreground Service

```kotlin
// GatewayService.kt
class GatewayService : Service() {
    val gateway: EmbeddedGateway
    val state: StateFlow<GatewayState>

    fun onReady(callback: () -> Unit)
    fun isGatewayRunning(): Boolean
}
```

Apps register this service in their `AndroidManifest.xml` and bind to it.

---

## 6. Protocol Specification

The SDK communicates with the ClawSeed gateway via two channels:

### 6.1 WebSocket Protocol (`/ws/chat`)

**Connection URL:** `ws://<host>:<port>/ws/chat?session_id=<id>&token=<token>`

**Protocol version:** `1` (sent in connect handshake)

**Authentication (priority order):**
1. `Authorization: Bearer <token>` header
2. `Sec-WebSocket-Protocol: bearer.<token>` subprotocol
3. `?token=<token>` query parameter

#### Client -> Server Messages

| Type | Fields | Description |
|------|--------|-------------|
| `connect` | `v: int` | Protocol handshake |
| `message` | `content: string, debug?: bool` | Send chat message |
| `register_tools` | `tools: ToolSpec[]` | Register client-side tools |
| `tool_result` | `id: string, output: string, success: bool` | Respond to tool_call_request |
| `tool_error` | `id: string, error: string` | Report tool execution failure |
| `get_registered_tools` | (none) | Query registered tools |

#### Server -> Client Messages

| Type | Fields | Description |
|------|--------|-------------|
| `session_start` | `v, session_id, name?, resumed, message_count` | Session established |
| `connected` | `v, message` | Connection acknowledged |
| `chunk` | `content: string` | Streaming text delta |
| `thinking` | `content: string` | Reasoning delta |
| `chunk_reset` | (none) | Flush accumulated content |
| `done` | `full_response: string` | Turn complete |
| `tool_call` | `id, name, args` | Server-side tool started |
| `tool_result` | `id, name, output` | Server-side tool completed |
| `tool_call_request` | `id, name, args` | Request client to execute tool |
| `tools_registered` | `count, registered` | Tools registration confirmed |
| `result_acknowledged` | `id` | Tool result received |
| `aborted` | (none) | Turn aborted |
| `title_updated` | `title` | Session title changed |
| `error` | `message, code?` | Error occurred |
| `debug_prompt` | `messages, estimated_tokens` | Debug info |

#### Error Codes

`AUTH_ERROR`, `PROVIDER_ERROR`, `AGENT_ERROR`, `INVALID_JSON`, `UNKNOWN_MESSAGE_TYPE`, `SESSION_BUSY`, `EMPTY_CONTENT`

### 6.2 REST API

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check (no auth) |
| POST | `/pair` | Exchange pairing code for bearer token |
| GET | `/api/status` | Gateway status (provider, model, etc.) |
| GET | `/api/config` | Get current TOML config |
| PUT | `/api/config` | Update TOML config |
| GET | `/api/tools` | List available tools |
| GET | `/api/sessions` | List all sessions |
| GET | `/api/sessions/{id}/messages` | Get session messages |
| GET | `/api/sessions/{id}/state` | Get session state |
| PUT | `/api/sessions/{id}` | Rename session |
| DELETE | `/api/sessions/{id}` | Delete session |
| POST | `/api/sessions/{id}/abort` | Abort current turn |
| GET | `/api/provider/models` | List available models |

---

## 7. Usage Examples

### 7.1 Pure JVM: Connect to Remote Gateway

```kotlin
fun main() = runBlocking {
    val session = ClawSeed.createSession(ClawSeedConfig(
        gatewayUrl = "http://192.168.1.100:3000",
        authToken = "zc_my_token",
    ))

    launch {
        session.events.collect { event ->
            when (event) {
                is ChatEvent.TextChunk -> print(event.content)
                is ChatEvent.Done -> println("\n[done]")
                is ChatEvent.Error -> println("\n[error] ${event.message}")
                else -> {}
            }
        }
    }

    session.connect()
    session.sendMessage("What is the weather today?")
}
```

### 7.2 Android App: With Custom Tools

```kotlin
// Application.onCreate()
ClawSeedAndroid.init(this, ClawSeedConfig(
    gatewayUrl = "http://my-server:3000",
    authToken = savedToken,
))

// In ViewModel or Activity
val session = ClawSeedAndroid.sessionManager().connect()

session.tools.register(
    name = "read_contacts",
    description = "Search phone contacts",
    parameters = """{"type":"object","properties":{"query":{"type":"string"}}}""",
) { args ->
    val query = args["query"]?.jsonPrimitive?.content ?: ""
    val contacts = contactsProvider.search(query)
    ToolResult.Success(Json.encodeToString(contacts))
}

session.tools.register(DeviceInfoTool(context))
session.sendMessage("Find John's phone number in my contacts")
```

### 7.3 Android App: With Embedded Gateway

```kotlin
class MyApp : Application() {
    private val appScope = CoroutineScope(Dispatchers.Main + SupervisorJob())

    override fun onCreate() {
        super.onCreate()
        val gateway = EmbeddedGateway(this)
        appScope.launch {
            gateway.start()  // blocks until gateway is ready
            ClawSeedAndroid.init(this@MyApp, gateway.localConfig())
        }
    }
}

// In ViewModel
class ChatViewModel(app: Application) : AndroidViewModel(app) {
    private val session = ClawSeedAndroid.sessionManager()
        .let { /* connect and collect events */ }
    // ... collect session.events, call session.sendMessage(), etc.
}
```
