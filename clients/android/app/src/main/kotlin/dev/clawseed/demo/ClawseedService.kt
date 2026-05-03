package dev.clawseed.demo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import dev.clawseed.client.ClawseedClient
import dev.clawseed.client.ToolCallHandler
import dev.clawseed.client.ToolCallResult
import dev.clawseed.client.ToolSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

enum class ConnState { DISCONNECTED, CONNECTING, CONNECTED }

data class SessionInfo(
    val sessionId: String,
    val name: String?,
    val resumed: Boolean,
    val messageCount: Int,
)

sealed class ChatLogEntry {
    data class System(val text: String) : ChatLogEntry()
    data class User(val text: String) : ChatLogEntry()
    data class Assistant(val text: String) : ChatLogEntry()
    data class ToolCall(val id: String, val name: String, val args: String) : ChatLogEntry()
    data class ToolResult(val id: String, val name: String, val output: String) : ChatLogEntry()
    data class DebugPrompt(val messages: String, val estimatedTokens: Int) : ChatLogEntry()
}

class ClawseedService : Service() {

    inner class LocalBinder : Binder() {
        val service: ClawseedService get() = this@ClawseedService
    }

    private val binder = LocalBinder()
    private val supervisorJob = SupervisorJob()
    private val scope = CoroutineScope(Dispatchers.IO + supervisorJob)

    private var process: Process? = null
    private var serviceJob: Job? = null

    private val readyCallbacks = mutableListOf<() -> Unit>()
    private var isReady = false

    // --- WS lifecycle management ---
    private var client: ClawseedClient? = null
    private val _connectionState = MutableStateFlow(ConnState.DISCONNECTED)
    val connectionState: StateFlow<ConnState> = _connectionState.asStateFlow()

    private val _sessionInfo = MutableStateFlow<SessionInfo?>(null)
    val sessionInfo: StateFlow<SessionInfo?> = _sessionInfo.asStateFlow()

    private val _streamingContent = MutableStateFlow("")
    val streamingContent: StateFlow<String> = _streamingContent.asStateFlow()

    private val _messages = MutableStateFlow<List<ChatLogEntry>>(emptyList())
    val messages: StateFlow<List<ChatLogEntry>> = _messages.asStateFlow()

    private val _thinkingContent = MutableStateFlow("")
    val thinkingContent: StateFlow<String> = _thinkingContent.asStateFlow()

    private val clientTools = mutableListOf<ToolSpec>()
    private var clientToolHandler: ToolCallHandler? = null

    fun connectSession(sessionId: String? = null) {
        // Always disconnect and clear state before new connection
        client?.disconnect()
        client = null
        _messages.value = emptyList()
        _streamingContent.value = ""
        _thinkingContent.value = ""
        _connectionState.value = ConnState.CONNECTING
        _sessionInfo.value = null
        hasAutoNamed = false

        val baseUrl = "ws://127.0.0.1:42617/ws/chat"
        val url = if (sessionId != null) "$baseUrl?session_id=$sessionId" else baseUrl

        val builder = ClawseedClient.builder(url)
        clientTools.forEach { builder.registerTool(it) }
        clientToolHandler?.let { builder.toolCallHandler(it) }

        val c = builder
            .onConnected {
                _connectionState.value = ConnState.CONNECTED
                appendMessage(ChatLogEntry.System("[已连接]"))
            }
            .onDisconnected {
                _connectionState.value = ConnState.DISCONNECTED
                appendMessage(ChatLogEntry.System("[已断开]"))
            }
            .onSessionStart { sid, name, resumed, count ->
                _sessionInfo.value = SessionInfo(sid, name, resumed, count)
                // Auto-name: use session name or first 20 chars of session ID
                val label = name ?: sid.take(8)
                appendMessage(ChatLogEntry.System("[会话: $label${if (resumed) " (恢复 $count 条)" else " (新建)"}]"))
            }
            .onChunk { chunk -> _streamingContent.value += chunk }
            .onChunkReset {
                if (_streamingContent.value.isNotEmpty()) {
                    appendMessage(ChatLogEntry.Assistant(_streamingContent.value))
                    _streamingContent.value = ""
                }
            }
            .onThinking { text -> _thinkingContent.value += text }
            .onDone { _ ->
                if (_streamingContent.value.isNotEmpty()) {
                    appendMessage(ChatLogEntry.Assistant(_streamingContent.value))
                    _streamingContent.value = ""
                }
                _thinkingContent.value = ""
                // Auto-rename session using first user message
                autoNameSessionIfNeeded()
            }
            .onToolCall { id, name, args ->
                appendMessage(ChatLogEntry.ToolCall(id, name, args.toString()))
            }
            .onToolResult { id, name, output ->
                appendMessage(ChatLogEntry.ToolResult(id, name, output))
            }
            .onAborted {
                appendMessage(ChatLogEntry.System("[已中止]"))
                _streamingContent.value = ""
            }
            .onError { err ->
                appendMessage(ChatLogEntry.System("[ERROR] $err"))
                _streamingContent.value = ""
            }
            .onDebugPrompt { messages, tokens ->
                appendMessage(ChatLogEntry.DebugPrompt(messages, tokens))
            }
            .build()

        c.connect()
        client = c
    }

    fun disconnect() {
        client?.disconnect()
        client = null
        _connectionState.value = ConnState.DISCONNECTED
        _sessionInfo.value = null
        _messages.value = emptyList()
        _streamingContent.value = ""
        _thinkingContent.value = ""
        hasAutoNamed = false
    }

    fun clearSession() {
        client?.disconnect()
        client = null
        _connectionState.value = ConnState.DISCONNECTED
        _sessionInfo.value = null
        _messages.value = emptyList()
        _streamingContent.value = ""
        _thinkingContent.value = ""
        hasAutoNamed = false
    }

    fun sendMessage(content: String, debug: Boolean = false) {
        if (content.isNotBlank()) {
            appendMessage(ChatLogEntry.User(content))
            client?.sendMessage(content, debug)
            // Auto-name: rename session from first user message
            autoNameSessionIfNeeded()
        }
    }

    private var hasAutoNamed = false
    private fun autoNameSessionIfNeeded() {
        if (hasAutoNamed) return
        val info = _sessionInfo.value ?: return
        if (info.name != null) { hasAutoNamed = true; return }
        val firstUserMsg = _messages.value.firstOrNull { it is ChatLogEntry.User } as? ChatLogEntry.User ?: return
        val name = firstUserMsg.text.take(20)
        hasAutoNamed = true
        scope.launch {
            try {
                val url = URL("http://127.0.0.1:42617/api/sessions/${info.sessionId}")
                val json = """{"name":"${name.replace("\"", "\\\"")}"}"""
                val conn = url.openConnection() as HttpURLConnection
                conn.requestMethod = "PUT"
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.outputStream.write(json.toByteArray())
                conn.responseCode
                conn.disconnect()
                _sessionInfo.value = info.copy(name = name)
            } catch (_: Exception) { }
        }
    }

    fun isConnected(): Boolean = _connectionState.value == ConnState.CONNECTED

    fun registerTool(spec: ToolSpec) {
        clientTools.add(spec)
    }

    fun setToolCallHandler(handler: ToolCallHandler) {
        clientToolHandler = handler
    }

    private fun appendMessage(entry: ChatLogEntry) {
        _messages.value = _messages.value + entry
    }

    override fun onBind(intent: Intent): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        startForeground(NOTIFICATION_ID, buildNotification("启动 clawseed gateway..."))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (process == null) {
            serviceJob = scope.launch { startGateway() }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceJob?.cancel()
        supervisorJob.cancel()
        process?.destroy()
        process = null
        isReady = false
    }

    fun onReady(callback: () -> Unit) {
        if (isReady) callback() else readyCallbacks.add(callback)
    }

    fun isGatewayRunning(): Boolean = process?.isAlive == true

    private suspend fun startGateway() {
        try {
            val binary = File(applicationInfo.nativeLibraryDir, "libclawseed.so")
            if (!binary.exists()) error("clawseed binary not found: ${binary.absolutePath}")
            Log.i(TAG, "Using binary: ${binary.absolutePath} (${binary.length()} bytes)")

            ensureConfig()
            updateNotification("Gateway 运行中 :42617")

            process = ProcessBuilder(binary.absolutePath, "gateway", "--port", "42617")
                .redirectErrorStream(true)
                .also { pb ->
                    pb.environment()["HOME"] = filesDir.absolutePath
                    pb.environment()["XDG_CONFIG_HOME"] = filesDir.absolutePath
                    pb.environment()["XDG_DATA_HOME"] = filesDir.absolutePath
                    val apiKeyFile = File(filesDir, ".clawseed/api_key")
                    if (apiKeyFile.exists()) {
                        val key = apiKeyFile.readText().trim()
                        if (key.isNotEmpty()) {
                            pb.environment()["CLAWSEED_API_KEY"] = key
                            Log.i(TAG, "API key loaded from file")
                        }
                    }
                }
                .start()

            scope.launch {
                process?.inputStream?.bufferedReader()?.forEachLine { line ->
                    Log.d(TAG, "clawseed: $line")
                }
            }

            waitUntilReady()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start clawseed gateway", e)
            updateNotification("启动失败: ${e.message}")
        }
    }

    private fun ensureConfig() {
        val configDir = File(filesDir, ".clawseed")
        configDir.mkdirs()

        // Ensure workspace directory exists so file tools can read/write
        val workspaceDir = File(configDir, "workspace")
        if (!workspaceDir.exists()) {
            workspaceDir.mkdirs()
            Log.i(TAG, "Created workspace directory: ${workspaceDir.absolutePath}")
        }

        val configFile = File(configDir, "clawseed.toml")
        if (configFile.exists()) {
            var content = configFile.readText()
            var changed = false

            // Ensure workspace_dir is set in config
            if (!content.contains("workspace_dir")) {
                content = "workspace_dir = \"${workspaceDir.absolutePath}\"\n$content"
                changed = true
                Log.i(TAG, "Added workspace_dir to config")
            }

            for ((section, patch) in WEB_FEATURE_PATCHES) {
                val patched = enableSectionIfPresent(content, section, patch)
                if (patched != content) {
                    content = patched
                    changed = true
                }
            }

            // Add missing web tool sections
            for ((header, body) in REQUIRED_SECTIONS) {
                if (!content.contains(header)) {
                    content = content.trimEnd() + "\n\n$header\n$body\n"
                    changed = true
                    Log.i(TAG, "Added missing section: $header")
                }
            }

            // Ensure web_search has provider set (for configs created before bing support)
            val wsIdx = content.indexOf("[web_search]")
            if (wsIdx != -1) {
                val nextSectionIdx = content.indexOf("\n[", wsIdx + 1).let { if (it == -1) content.length else it }
                val wsSection = content.substring(wsIdx, nextSectionIdx)
                if (!wsSection.contains("provider")) {
                    content = content.substring(0, wsIdx) +
                        wsSection.replace("[web_search]\n", "[web_search]\nprovider = \"bing\"\n") +
                        content.substring(nextSectionIdx)
                    changed = true
                    Log.i(TAG, "Added provider = bing to [web_search]")
                }
            }

            if (changed) configFile.writeText(content)
        } else {
            configFile.writeText(INITIAL_CONFIG.replace("{WORKSPACE_DIR}", workspaceDir.absolutePath))
            Log.i(TAG, "Created initial config")
        }
    }

    private fun enableSectionIfPresent(content: String, sectionHeader: String, patch: Pair<String, String>): String {
        val sectionIdx = content.indexOf("\n$sectionHeader\n")
        if (sectionIdx == -1) return content
        val nextSection = content.indexOf("\n[", sectionIdx + 1).let { if (it == -1) content.length else it }
        val before = content.substring(0, sectionIdx)
        var section = content.substring(sectionIdx, nextSection)
        val after = content.substring(nextSection)
        if (section.contains(patch.first)) {
            section = section.replace(patch.first, patch.second)
            Log.i(TAG, "Patched config: $sectionHeader ${patch.second}")
        }
        // Ensure allowed_domains is present for network tool sections
        if (sectionHeader in listOf("[http_request]", "[web_fetch]") && !section.contains("allowed_domains")) {
            section = section.trimEnd() + "\nallowed_domains = [\"*\"]\n"
            Log.i(TAG, "Added allowed_domains to $sectionHeader")
        }
        return before + section + after
    }

    companion object {
        private const val TAG = "ClawseedService"
        private const val CHANNEL_ID = "clawseed_gateway"
        private const val NOTIFICATION_ID = 1001
        private const val MAX_HEALTH_ATTEMPTS = 40
        private const val HEALTH_POLL_MS = 500L

        private val WEB_FEATURE_PATCHES = listOf(
            "[web_fetch]"    to ("enabled = false" to "enabled = true"),
            "[http_request]" to ("enabled = false" to "enabled = true"),
            "[web_search]"   to ("enabled = false" to "enabled = true"),
        )

        private val REQUIRED_SECTIONS = listOf(
            "[web_fetch]" to "enabled = true\nallowed_domains = [\"*\"]",
            "[http_request]" to "enabled = true\nallowed_domains = [\"*\"]",
            "[web_search]" to "enabled = true\nprovider = \"bing\"",
        )

        private val INITIAL_CONFIG = """
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
""".trimIndent() + "\n"
    }

    private suspend fun waitUntilReady() {
        val healthUrl = "http://127.0.0.1:42617/health"
        repeat(MAX_HEALTH_ATTEMPTS) {
            val code = withContext(Dispatchers.IO) {
                try {
                    val conn = URL(healthUrl).openConnection() as HttpURLConnection
                    conn.connectTimeout = 500
                    conn.readTimeout = 500
                    val result = conn.responseCode
                    conn.disconnect()
                    result
                } catch (_: Exception) {
                    -1
                }
            }
            if (code in 200..299) {
                isReady = true
                Log.i(TAG, "clawseed gateway ready")
                updateNotification("Gateway 已就绪 :42617")
                withContext(Dispatchers.Main) {
                    readyCallbacks.forEach { it() }
                    readyCallbacks.clear()
                }
                return
            }
            delay(HEALTH_POLL_MS)
        }
        Log.w(TAG, "Gateway did not become ready in time")
        updateNotification("Gateway 无响应 — 查看 logcat")
        stopSelf()
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID, "ClawSeed Gateway",
            NotificationManager.IMPORTANCE_LOW,
        ).apply { description = "ClawSeed gateway service" }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun buildNotification(text: String): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("ClawSeed")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setOngoing(true)
            .build()

    private fun updateNotification(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIFICATION_ID, buildNotification(text))
    }
}
