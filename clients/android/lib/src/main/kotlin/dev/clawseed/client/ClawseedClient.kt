package dev.clawseed.client

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

fun interface ToolCallHandler {
    fun handleToolCall(request: ToolCallRequest): ToolCallResult
}

class ClawseedClient private constructor(
    private val url: String,
    private val authToken: String?,
    private val tools: List<ToolSpec>,
    private val toolCallHandler: ToolCallHandler?,
    private val onConnected: (() -> Unit)?,
    private val onDisconnected: (() -> Unit)?,
    private val onChunk: ((String) -> Unit)?,
    private val onThinking: ((String) -> Unit)?,
    private val onDone: ((String) -> Unit)?,
    private val onToolCall: ((id: String, name: String, args: JSONObject) -> Unit)?,
    private val onToolResult: ((id: String, name: String, output: String) -> Unit)?,
    private val onAborted: (() -> Unit)?,
    private val onError: ((String) -> Unit)?,
) {
    private val httpClient = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()

    private val executor = Executors.newSingleThreadExecutor()

    @Volatile private var webSocket: WebSocket? = null

    fun connect() {
        val reqBuilder = Request.Builder().url(url)
        authToken?.let { reqBuilder.addHeader("Authorization", "Bearer $it") }
        webSocket = httpClient.newWebSocket(reqBuilder.build(), Listener())
    }

    fun disconnect() {
        webSocket?.close(1000, null)
        webSocket = null
        executor.shutdown()
        httpClient.connectionPool.evictAll()
    }

    fun sendMessage(content: String) {
        val json = JSONObject().put("type", "message").put("content", content)
        webSocket?.send(json.toString())
    }

    private fun registerTools() {
        val arr = JSONArray()
        tools.forEach { arr.put(it.toJson()) }
        val json = JSONObject().put("type", "register_tools").put("tools", arr)
        webSocket?.send(json.toString())
    }

    private fun dispatchToolCall(request: ToolCallRequest) {
        val handler = toolCallHandler ?: run {
            val err = ToolCallResult.Failure("No tool handler registered")
            webSocket?.send(err.toJson(request.id).toString())
            return
        }
        if (executor.isShutdown) return
        executor.execute {
            val result = runCatching { handler.handleToolCall(request) }
                .getOrElse { ToolCallResult.Failure(it.message ?: "Handler threw exception") }
            webSocket?.send(result.toJson(request.id).toString())
        }
    }

    private inner class Listener : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            if (tools.isNotEmpty()) registerTools()
            onConnected?.invoke()
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            when (val msg = IncomingMessage.parse(text)) {
                is IncomingMessage.SessionStart -> Unit
                is IncomingMessage.Connected -> Unit
                is IncomingMessage.Chunk -> onChunk?.invoke(msg.text)
                is IncomingMessage.Thinking -> onThinking?.invoke(msg.text)
                is IncomingMessage.Done -> {
                    onChunk?.invoke("") // signal end of streaming
                    onDone?.invoke(msg.finalText)
                }
                is IncomingMessage.ToolCallMsg -> onToolCall?.invoke(msg.id, msg.name, msg.args)
                is IncomingMessage.ToolResultMsg -> onToolResult?.invoke(msg.id, msg.name, msg.output)
                is IncomingMessage.ToolCallRequestMsg -> dispatchToolCall(msg.request)
                is IncomingMessage.ToolsRegistered -> Unit
                is IncomingMessage.ResultAcknowledged -> Unit
                is IncomingMessage.ChunkReset -> Unit
                is IncomingMessage.Aborted -> onAborted?.invoke()
                is IncomingMessage.Error -> onError?.invoke(msg.message)
                null -> Unit
            }
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(1000, null)
            onDisconnected?.invoke()
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            this@ClawseedClient.webSocket = null
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            this@ClawseedClient.webSocket = null
            onDisconnected?.invoke()
            onError?.invoke(t.message ?: "WebSocket failure")
        }
    }

    class Builder(private val url: String) {
        private var authToken: String? = null
        private val tools = mutableListOf<ToolSpec>()
        private var toolCallHandler: ToolCallHandler? = null
        private var onConnected: (() -> Unit)? = null
        private var onDisconnected: (() -> Unit)? = null
        private var onChunk: ((String) -> Unit)? = null
        private var onThinking: ((String) -> Unit)? = null
        private var onDone: ((String) -> Unit)? = null
        private var onToolCall: ((id: String, name: String, args: JSONObject) -> Unit)? = null
        private var onToolResult: ((id: String, name: String, output: String) -> Unit)? = null
        private var onAborted: (() -> Unit)? = null
        private var onError: ((String) -> Unit)? = null

        fun authToken(token: String) = apply { authToken = token }
        fun registerTool(tool: ToolSpec) = apply { tools.add(tool) }
        fun toolCallHandler(handler: ToolCallHandler) = apply { toolCallHandler = handler }
        fun onConnected(callback: () -> Unit) = apply { onConnected = callback }
        fun onDisconnected(callback: () -> Unit) = apply { onDisconnected = callback }
        fun onChunk(callback: (String) -> Unit) = apply { onChunk = callback }
        fun onThinking(callback: (String) -> Unit) = apply { onThinking = callback }
        fun onDone(callback: (String) -> Unit) = apply { onDone = callback }
        fun onToolCall(callback: (id: String, name: String, args: JSONObject) -> Unit) = apply { onToolCall = callback }
        fun onToolResult(callback: (id: String, name: String, output: String) -> Unit) = apply { onToolResult = callback }
        fun onAborted(callback: () -> Unit) = apply { onAborted = callback }
        fun onError(callback: (String) -> Unit) = apply { onError = callback }

        fun build() = ClawseedClient(
            url = url,
            authToken = authToken,
            tools = tools.toList(),
            toolCallHandler = toolCallHandler,
            onConnected = onConnected,
            onDisconnected = onDisconnected,
            onChunk = onChunk,
            onThinking = onThinking,
            onDone = onDone,
            onToolCall = onToolCall,
            onToolResult = onToolResult,
            onAborted = onAborted,
            onError = onError,
        )
    }

    companion object {
        fun builder(url: String) = Builder(url)
    }
}
