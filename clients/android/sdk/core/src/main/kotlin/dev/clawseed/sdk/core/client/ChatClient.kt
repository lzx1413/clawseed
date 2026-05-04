package dev.clawseed.sdk.core.client

import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.tool.ToolRegistry
import dev.clawseed.sdk.core.tool.ToolResult
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.math.min
import kotlin.random.Random
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.suspendCancellableCoroutine

internal class ChatClient(
    private val url: String,
    private val authTokenProvider: () -> String?,
    private val toolRegistry: ToolRegistry,
    private val reconnectPolicy: ReconnectPolicy,
    private val json: Json = Json { ignoreUnknownKeys = true },
) {
    private val httpClient = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val _connectionState = MutableStateFlow(ConnectionState.DISCONNECTED)
    val connectionState: StateFlow<ConnectionState> = _connectionState.asStateFlow()

    private val _events = MutableSharedFlow<ChatEvent>(extraBufferCapacity = 32)
    val events: SharedFlow<ChatEvent> = _events.asSharedFlow()

    @Volatile private var webSocket: WebSocket? = null
    @Volatile private var sessionId: String? = null
    @Volatile private var intentionalDisconnect = false
    @Volatile private var reconnectAttempt = 0
    private val pendingMessages = ConcurrentLinkedQueue<String>()
    private val connectLock = Any()
    private val connectWaiters = mutableListOf<CancellableContinuation<Unit>>()

    init {
        toolRegistry.onToolRegistered {
            if (_connectionState.value == ConnectionState.CONNECTED) {
                registerTools()
            }
        }
    }

    suspend fun connect(sessionId: String? = null) {
        val targetSessionId = resolveSessionId(sessionId, this.sessionId)
        val currentState = _connectionState.value
        // If already connected/connecting to the same session, no-op or await the in-flight connect.
        if ((currentState == ConnectionState.CONNECTED ||
            currentState == ConnectionState.CONNECTING ||
            currentState == ConnectionState.RECONNECTING) &&
            this.sessionId == targetSessionId
        ) {
            if (currentState == ConnectionState.CONNECTED) {
                return
            }
            awaitConnected()
            return
        }
        // Disconnect if switching sessions
        if (this.sessionId != null && this.sessionId != targetSessionId && currentState != ConnectionState.DISCONNECTED) {
            disconnect()
        }
        this.sessionId = targetSessionId
        intentionalDisconnect = false
        reconnectAttempt = 0
        _connectionState.value = ConnectionState.CONNECTING
        awaitConnected {
            openWebSocket()
        }
    }

    fun disconnect() {
        intentionalDisconnect = true
        _connectionState.value = ConnectionState.DISCONNECTED
        webSocket?.close(1000, null)
        webSocket = null
        pendingMessages.clear()
    }

    fun sendMessage(content: String, debug: Boolean = false) {
        val state = _connectionState.value
        check(state == ConnectionState.CONNECTED || state == ConnectionState.RECONNECTING) {
            "Cannot send message in state $state"
        }
        val msg = buildJsonObject {
            put("type", "message")
            put("content", content)
            if (debug) put("debug", true)
        }.toString()
        if (state == ConnectionState.CONNECTED) {
            webSocket?.send(msg)
        } else {
            pendingMessages.add(msg)
        }
    }

    fun sendAbort(sessionId: String) {
        val msg = buildJsonObject {
            put("type", "abort")
        }.toString()
        webSocket?.send(msg)
    }

    private fun openWebSocket() {
        val wsUrl = if (sessionId != null) {
            val separator = if ("?" in url) "&" else "?"
            "$url${separator}session_id=$sessionId"
        } else {
            url
        }
        val reqBuilder = Request.Builder().url(wsUrl)
        authTokenProvider()?.let { reqBuilder.addHeader("Authorization", "Bearer $it") }
        webSocket = httpClient.newWebSocket(reqBuilder.build(), WsListener())
    }

    private suspend fun awaitConnected(startConnection: (() -> Unit)? = null) {
        suspendCancellableCoroutine { continuation ->
            var shouldStartConnection = false
            synchronized(connectLock) {
                if (_connectionState.value == ConnectionState.CONNECTED) {
                    continuation.resume(Unit)
                    return@suspendCancellableCoroutine
                }
                connectWaiters.add(continuation)
                shouldStartConnection = startConnection != null
            }

            continuation.invokeOnCancellation {
                synchronized(connectLock) {
                    connectWaiters.remove(continuation)
                }
            }

            if (shouldStartConnection) {
                runCatching { startConnection?.invoke() }
                    .onFailure { error ->
                        failPendingConnects(error)
                    }
            }
        }
    }

    private fun completePendingConnects() {
        val waiters = synchronized(connectLock) {
            connectWaiters.toList().also { connectWaiters.clear() }
        }
        waiters.forEach { waiter ->
            if (waiter.isActive) {
                waiter.resume(Unit)
            }
        }
    }

    private fun failPendingConnects(error: Throwable) {
        val waiters = synchronized(connectLock) {
            connectWaiters.toList().also { connectWaiters.clear() }
        }
        waiters.forEach { waiter ->
            if (waiter.isActive) {
                waiter.resumeWithException(error)
            }
        }
    }

    private fun registerTools() {
        val tools = toolRegistry.registeredTools()
        if (tools.isEmpty()) return
        val arr = buildJsonArray {
            for (tool in tools) {
                add(buildJsonObject {
                    put("name", tool.name)
                    put("description", tool.description)
                    put("parameters", tool.parameters)
                })
            }
        }
        val msg = buildJsonObject {
            put("type", "register_tools")
            put("tools", arr)
        }.toString()
        webSocket?.send(msg)
    }

    private fun dispatchToolCall(id: String, name: String, args: JsonObject) {
        val tool = toolRegistry.get(name)
        if (tool == null) {
            sendToolError(id, "Unknown tool: $name")
            return
        }
        scope.launch {
            val result = runCatching { tool.execute(args) }
                .getOrElse { ToolResult.Failure(it.message ?: "Handler threw exception") }
            when (result) {
                is ToolResult.Success -> sendToolResult(id, result.output)
                is ToolResult.Failure -> sendToolError(id, result.error)
            }
        }
    }

    private fun sendToolResult(id: String, output: String) {
        val msg = buildJsonObject {
            put("type", "tool_result")
            put("id", id)
            put("output", output)
            put("success", true)
        }.toString()
        webSocket?.send(msg)
    }

    private fun sendToolError(id: String, error: String) {
        val msg = buildJsonObject {
            put("type", "tool_error")
            put("id", id)
            put("error", error)
            put("success", false)
        }.toString()
        webSocket?.send(msg)
    }

    private suspend fun handleReconnect() {
        val policy = reconnectPolicy
        if (policy is ReconnectPolicy.None || intentionalDisconnect) {
            _connectionState.value = ConnectionState.DISCONNECTED
            failPendingConnects(IllegalStateException("WebSocket reconnect is disabled or was cancelled"))
            return
        }
        val backoff = policy as ReconnectPolicy.ExponentialBackoff
        _connectionState.value = ConnectionState.RECONNECTING
        while (reconnectAttempt < backoff.maxAttempts && !intentionalDisconnect) {
            val baseDelay = min(
                backoff.initialDelayMs * (1L shl reconnectAttempt),
                backoff.maxDelayMs
            )
            val jitter = (baseDelay * Random.nextDouble(0.0, 0.5)).toLong()
            delay(baseDelay + jitter)
            reconnectAttempt++
            _events.emit(ChatEvent.Error("Reconnecting (attempt $reconnectAttempt)..."))
            openWebSocket()
            return
        }
        _connectionState.value = ConnectionState.DISCONNECTED
        failPendingConnects(IllegalStateException("WebSocket reconnect attempts exhausted"))
    }

    private inner class WsListener : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            val connectMsg = buildJsonObject {
                put("type", "connect")
                put("v", PROTOCOL_VERSION)
                sessionId?.let { put("session_id", it) }
            }.toString()
            webSocket.send(connectMsg)
            registerTools()
            _connectionState.value = ConnectionState.CONNECTED
            reconnectAttempt = 0
            completePendingConnects()
            scope.launch {
                _events.emit(ChatEvent.Connected("WebSocket connected", PROTOCOL_VERSION))
            }
            // Flush any queued messages
            while (pendingMessages.isNotEmpty()) {
                val msg = pendingMessages.poll() ?: break
                webSocket.send(msg)
            }
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            val event = ChatEvent.parse(text, json) ?: return
            if (event is ChatEvent.SessionStarted) {
                sessionId = event.sessionId
            }
            scope.launch {
                _events.emit(event)
            }
            if (event is ChatEvent.ToolCallRequested) {
                dispatchToolCall(event.id, event.name, event.args)
            }
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(1000, null)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            this@ChatClient.webSocket = null
            if (!shouldReconnectOnClose(code, intentionalDisconnect)) {
                _connectionState.value = ConnectionState.DISCONNECTED
                failPendingConnects(IllegalStateException("WebSocket closed: $code${if (reason.isNotBlank()) " ($reason)" else ""}"))
                return
            }
            scope.launch {
                _events.emit(ChatEvent.Error("WebSocket closed: $code${if (reason.isNotBlank()) " ($reason)" else ""}"))
                handleReconnect()
            }
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            this@ChatClient.webSocket = null
            if (intentionalDisconnect) {
                failPendingConnects(IllegalStateException("WebSocket connection was cancelled", t))
                return
            }
            if (reconnectPolicy is ReconnectPolicy.None) {
                failPendingConnects(t)
            }
            scope.launch {
                _events.emit(ChatEvent.Error(t.message ?: "WebSocket failure"))
                handleReconnect()
            }
        }
    }

    companion object {
        internal const val PROTOCOL_VERSION = 1

        internal fun resolveSessionId(requestedSessionId: String?, currentSessionId: String?): String? {
            return requestedSessionId ?: currentSessionId
        }

        internal fun shouldReconnectOnClose(code: Int, intentionalDisconnect: Boolean): Boolean {
            return !intentionalDisconnect && code != 1000
        }
    }
}
