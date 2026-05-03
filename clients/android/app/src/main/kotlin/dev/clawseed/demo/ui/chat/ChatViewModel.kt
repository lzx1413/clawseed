package dev.clawseed.demo.ui.chat

import android.Manifest
import android.app.Application
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Build
import android.os.IBinder
import androidx.core.content.ContextCompat
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.client.ToolCallResult
import dev.clawseed.client.ToolSpec
import dev.clawseed.demo.ChatLogEntry
import dev.clawseed.demo.ClawseedService
import dev.clawseed.demo.ConnState
import dev.clawseed.demo.CoordinateConverter
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.GatewayApi
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.data.TurnState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.util.Locale

data class ChatUiState(
    val messages: List<ChatEntry> = emptyList(),
    val streamingContent: String = "",
    val thinkingContent: String = "",
    val turnState: TurnState = TurnState.IDLE,
    val connState: ConnState = ConnState.DISCONNECTED,
    val sessionName: String? = null,
    val currentSessionId: String? = null,
    val error: String? = null,
)

class ChatViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    private var service: ClawseedService? = null
    private val messages = mutableListOf<ChatEntry>()
    private val api = GatewayApi()
    private val localStore = LocalStore(application)
    private var debugEnabled = false
    private var historyLoaded = false
    private var pendingSessionId: String? = UNSET
    private var toolsRegistered = false

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            service = (binder as ClawseedService.LocalBinder).service
            setupServiceObservers()
            if (pendingSessionId !== UNSET) {
                val sid = pendingSessionId
                pendingSessionId = UNSET
                doConnect(sid)
            }
        }
        override fun onServiceDisconnected(name: ComponentName) {
            service = null
            _uiState.value = _uiState.value.copy(connState = ConnState.DISCONNECTED)
        }
    }

    init {
        val intent = Intent(application, ClawseedService::class.java)
        application.bindService(intent, serviceConnection, Context.BIND_AUTO_CREATE)
        viewModelScope.launch {
            localStore.showDebugInfo.collect { debugEnabled = it }
        }
    }

    private fun setupServiceObservers() {
        val svc = service ?: return
        viewModelScope.launch {
            svc.connectionState.collect { state ->
                val error = if (state == ConnState.CONNECTED) null else _uiState.value.error
                _uiState.value = _uiState.value.copy(connState = state, error = error)
            }
        }
        viewModelScope.launch {
            svc.messages.collect { entries ->
                syncMessages(entries)
            }
        }
        viewModelScope.launch {
            svc.streamingContent.collect { content ->
                val isStreaming = content.isNotEmpty()
                _uiState.value = _uiState.value.copy(
                    streamingContent = content,
                    turnState = if (isStreaming) TurnState.STREAMING_TEXT else _uiState.value.turnState,
                )
            }
        }
        viewModelScope.launch {
            svc.thinkingContent.collect { content ->
                _uiState.value = _uiState.value.copy(thinkingContent = content)
            }
        }
        viewModelScope.launch {
            svc.sessionInfo.collect { info ->
                _uiState.value = _uiState.value.copy(
                    sessionName = info?.name,
                    currentSessionId = info?.sessionId,
                )
            }
        }
    }

    private fun syncMessages(entries: List<ChatLogEntry>) {
        val newMessages = mutableListOf<ChatEntry>()
        var latestError: String? = null
        for (entry in entries) {
            when (entry) {
                is ChatLogEntry.User -> newMessages.add(
                    ChatEntry.UserMessage(
                        id = "svc-${newMessages.size}",
                        timestamp = System.currentTimeMillis(),
                        content = entry.text,
                    )
                )
                is ChatLogEntry.Assistant -> newMessages.add(
                    ChatEntry.AssistantMessage(
                        id = "svc-${newMessages.size}",
                        timestamp = System.currentTimeMillis(),
                        content = entry.text,
                    )
                )
                is ChatLogEntry.ToolCall -> newMessages.add(
                    ChatEntry.ToolCall(
                        id = "svc-${newMessages.size}",
                        timestamp = System.currentTimeMillis(),
                        toolCallId = entry.id,
                        toolName = entry.name,
                        toolArgs = entry.args,
                    )
                )
                is ChatLogEntry.ToolResult -> newMessages.add(
                    ChatEntry.ToolResult(
                        id = "svc-${newMessages.size}",
                        timestamp = System.currentTimeMillis(),
                        toolCallId = entry.id,
                        toolName = entry.name,
                        toolResult = entry.output,
                        toolSuccess = true,
                    )
                )
                is ChatLogEntry.System -> {
                    if (entry.text.startsWith("[ERROR]")) {
                        latestError = entry.text.removePrefix("[ERROR] ")
                    }
                }
                is ChatLogEntry.DebugPrompt -> newMessages.add(
                    ChatEntry.DebugInfo(
                        id = "svc-${newMessages.size}",
                        timestamp = System.currentTimeMillis(),
                        messagesJson = entry.messages,
                        estimatedTokens = entry.estimatedTokens,
                    )
                )
                is ChatLogEntry.Thinking -> newMessages.add(
                    ChatEntry.Thinking(
                        id = "svc-${newMessages.size}",
                        timestamp = System.currentTimeMillis(),
                        content = entry.text,
                    )
                )
            }
        }
        messages.clear()
        if (historyLoaded) {
            val historyMessages = _uiState.value.messages.filter { it.id.startsWith("hist-") }
            messages.addAll(historyMessages)
        }
        messages.addAll(newMessages)
        _uiState.value = _uiState.value.copy(
            messages = messages.toList(),
            error = latestError ?: _uiState.value.error,
        )
    }

    fun switchToSession(sessionId: String?) {
        historyLoaded = false
        messages.clear()
        _uiState.value = _uiState.value.copy(
            messages = emptyList(),
            streamingContent = "",
            thinkingContent = "",
            sessionName = null,
            error = null,
        )

        val svc = service
        if (svc == null) {
            pendingSessionId = sessionId
            return
        }
        doConnect(sessionId)
    }

    private fun doConnect(sessionId: String?) {
        val svc = service ?: return
        if (!toolsRegistered) {
            svc.registerTool(
                ToolSpec(
                    name = "device_info",
                    description = "获取Android设备信息，包括型号、制造商、Android版本",
                    parameters = """{"type":"object","properties":{},"required":[]}""",
                )
            )
            svc.registerTool(
                ToolSpec(
                    name = "get_location",
                    description = "获取用户当前的地理位置信息，包括经纬度和城市名称。当用户询问天气、附近地点、本地服务等需要位置信息的问题时使用此工具。",
                    parameters = """{"type":"object","properties":{},"required":[]}""",
                )
            )
            svc.setToolCallHandler { request ->
                when (request.name) {
                    "device_info" -> {
                        val info = JSONObject()
                            .put("model", Build.MODEL)
                            .put("manufacturer", Build.MANUFACTURER)
                            .put("android_version", Build.VERSION.RELEASE)
                            .put("sdk_int", Build.VERSION.SDK_INT)
                        ToolCallResult.Success(info.toString())
                    }
                    "get_location" -> handleGetLocation()
                    else -> ToolCallResult.Failure("Unknown tool: ${request.name}")
                }
            }
            toolsRegistered = true
        }

        if (sessionId != null) {
            viewModelScope.launch {
                loadHistory(sessionId)
                svc.connectSession(sessionId)
            }
        } else {
            svc.connectSession(null)
        }
    }

    private suspend fun loadHistory(sessionId: String) {
        api.getSessionMessages(sessionId)
            .onSuccess { msgs ->
                historyLoaded = true
                messages.clear()
                for ((idx, msg) in msgs.withIndex()) {
                    when (msg.role) {
                        "user" -> messages.add(
                            ChatEntry.UserMessage(
                                id = "hist-$idx",
                                timestamp = System.currentTimeMillis(),
                                content = msg.content ?: "",
                            )
                        )
                        "assistant" -> messages.add(
                            ChatEntry.AssistantMessage(
                                id = "hist-$idx",
                                timestamp = System.currentTimeMillis(),
                                content = msg.content ?: "",
                            )
                        )
                    }
                }
                _uiState.value = _uiState.value.copy(messages = messages.toList())
            }
            .onFailure { /* history load failed, proceed without it */ }
    }

    fun sendMessage(content: String) {
        if (content.isNotBlank()) {
            service?.sendMessage(content, debugEnabled)
        }
    }

    fun abortGeneration() {
        val sessionId = _uiState.value.currentSessionId ?: return
        viewModelScope.launch {
            api.abortSession(sessionId)
        }
    }

    fun clearError() {
        _uiState.value = _uiState.value.copy(error = null)
    }

    override fun onCleared() {
        super.onCleared()
        getApplication<Application>().unbindService(serviceConnection)
    }

    @Suppress("MissingPermission")
    private fun handleGetLocation(): ToolCallResult {
        val ctx = getApplication<Application>()

        val hasPermission = ContextCompat.checkSelfPermission(
            ctx, Manifest.permission.ACCESS_FINE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED || ContextCompat.checkSelfPermission(
            ctx, Manifest.permission.ACCESS_COARSE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED

        if (!hasPermission) {
            return ToolCallResult.Failure("位置权限未授予，请在系统设置中允许ClawSeed访问位置信息")
        }

        val locationManager = ctx.getSystemService(Context.LOCATION_SERVICE) as LocationManager

        val providers = listOf(
            LocationManager.GPS_PROVIDER,
            LocationManager.NETWORK_PROVIDER,
            LocationManager.PASSIVE_PROVIDER,
        )

        var bestLocation: Location? = null
        for (provider in providers) {
            if (!locationManager.isProviderEnabled(provider)) continue
            val loc = try { locationManager.getLastKnownLocation(provider) } catch (_: Exception) { null }
            if (loc != null && (bestLocation == null || loc.time > bestLocation.time)) {
                bestLocation = loc
            }
        }

        if (bestLocation == null) {
            return ToolCallResult.Failure("无法获取位置信息，请确保GPS或网络定位已开启")
        }

        val gcj02 = CoordinateConverter.wgs84ToGcj02(bestLocation.latitude, bestLocation.longitude)

        val result = JSONObject()
            .put("latitude", gcj02.latitude)
            .put("longitude", gcj02.longitude)
            .put("accuracy_meters", bestLocation.accuracy.toDouble())
            .put("provider", bestLocation.provider)

        try {
            val geocoder = Geocoder(ctx, Locale.getDefault())
            @Suppress("DEPRECATION")
            val addresses = geocoder.getFromLocation(bestLocation.latitude, bestLocation.longitude, 1)
            if (!addresses.isNullOrEmpty()) {
                val addr = addresses[0]
                addr.locality?.let { result.put("city", it) }
                addr.adminArea?.let { result.put("province", it) }
                addr.subLocality?.let { result.put("district", it) }
                addr.getAddressLine(0)?.let { result.put("address", it) }
            }
        } catch (_: Exception) {
            // Geocoder not available on this device, return coordinates only
        }

        return ToolCallResult.Success(result.toString())
    }

    companion object {
        private val UNSET: String? = " UNSET"
    }
}
