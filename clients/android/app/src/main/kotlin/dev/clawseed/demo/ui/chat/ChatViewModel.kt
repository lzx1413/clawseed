package dev.clawseed.demo.ui.chat

import android.Manifest
import android.app.Application
import android.content.Context
import android.content.pm.PackageManager
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.data.TurnState
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.android.ChatAccumulator
import dev.clawseed.sdk.android.cetp.AuthRequiredEvent
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolResult
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.util.Locale

data class AuthPrompt(
    val hint: String,
    val authorizeIntent: String?,
)

data class ChatUiState(
    val messages: List<ChatEntry> = emptyList(),
    val streamingContent: String = "",
    val thinkingContent: String = "",
    val turnState: TurnState = TurnState.IDLE,
    val connState: ConnectionState = ConnectionState.DISCONNECTED,
    val sessionName: String? = null,
    val currentSessionId: String? = null,
    val error: String? = null,
    val authPrompt: AuthPrompt? = null,
)

class ChatViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    private val localStore = LocalStore(application)
    private var debugEnabled = false
    private var historyLoaded = false
    private var toolsRegistered = false
    private var accumulator: ChatAccumulator? = null
    private var currentSession: ClawSeedSession? = null
    private var connectJob: Job? = null
    private var accumulatorObservationJob: Job? = null
    private var sessionObservationJob: Job? = null
    private var authEventJob: Job? = null

    init {
        viewModelScope.launch {
            localStore.showDebugInfo.collect { debugEnabled = it }
        }
    }

    private fun sessionManager(): dev.clawseed.sdk.android.SessionManager {
        return ClawSeedAndroid.sessionManager()
    }

    fun switchToSession(sessionId: String?) {
        // If already connected to the same session (e.g. after config change), skip reconnection
        val currentSid = currentSession?.sessionInfo?.value?.sessionId
        if (currentSid != null && currentSid == sessionId
            && currentSession?.connectionState?.value == ConnectionState.CONNECTED
        ) {
            return
        }
        connectJob?.cancel()
        accumulatorObservationJob?.cancel()
        sessionObservationJob?.cancel()
        historyLoaded = false
        accumulator?.reset()
        currentSession = null
        _uiState.value = _uiState.value.copy(
            messages = emptyList(),
            streamingContent = "",
            thinkingContent = "",
            turnState = TurnState.IDLE,
            connState = ConnectionState.DISCONNECTED,
            sessionName = null,
            currentSessionId = null,
            error = null,
        )
        doConnect(sessionId)
    }

    private fun doConnect(sessionId: String?) {
        connectJob?.cancel()
        connectJob = viewModelScope.launch {
            try {
                ClawSeedAndroid.awaitInit()
                val session = sessionManager().connect(sessionId)
                currentSession = session

                if (!toolsRegistered) {
                    registerTools(session)
                    toolsRegistered = true
                }

                if (sessionId != null) {
                    loadHistory(session, sessionId)
                }

                // Disconnect previous accumulator
                accumulator?.reset()
                accumulatorObservationJob?.cancel()
                sessionObservationJob?.cancel()

                // Set up new accumulator
                val acc = ChatAccumulator(session)
                acc.startIn(viewModelScope)
                accumulator = acc

                // Observe accumulator state
                observeAccumulator(acc)
                observeConnectionState(session)
                observeAuthEvents()
            } catch (e: Exception) {
                _uiState.value = _uiState.value.copy(error = e.message)
            }
        }
    }

    private fun registerTools(session: ClawSeedSession) {
        session.tools.register(
            name = "device_info",
            description = "获取Android设备信息，包括型号、制造商、Android版本",
            parameters = """{"type":"object","properties":{},"required":[]}""",
        ) { _ ->
            val info = buildJsonObject {
                put("model", Build.MODEL)
                put("manufacturer", Build.MANUFACTURER)
                put("android_version", Build.VERSION.RELEASE)
                put("sdk_int", Build.VERSION.SDK_INT)
            }
            ToolResult.Success(info.toString())
        }

        session.tools.register(
            name = "get_location",
            description = "获取用户当前的地理位置信息，包括经纬度和城市名称。当用户询问天气、附近地点、本地服务等需要位置信息的问题时使用此工具。",
            parameters = """{"type":"object","properties":{},"required":[]}""",
        ) { _ ->
            handleGetLocation()
        }
    }

    private suspend fun loadHistory(session: ClawSeedSession, sessionId: String) {
        session.gateway.sessionMessages(sessionId)
            .onSuccess { msgs ->
                historyLoaded = true
                val historyEntries = msgs.mapIndexed { idx, msg ->
                    when (msg.role) {
                        "user" -> ChatEntry.UserMessage(
                            id = "hist-$idx",
                            timestamp = System.currentTimeMillis(),
                            content = msg.content ?: "",
                        )
                        "assistant" -> ChatEntry.AssistantMessage(
                            id = "hist-$idx",
                            timestamp = System.currentTimeMillis(),
                            content = msg.content ?: "",
                        )
                        else -> null
                    }
                }.filterNotNull()
                _uiState.value = _uiState.value.copy(messages = historyEntries)
            }
    }

    private fun observeAccumulator(acc: ChatAccumulator) {
        accumulatorObservationJob?.cancel()
        accumulatorObservationJob = viewModelScope.launch {
            launch {
                acc.streamingContent.collect { content ->
                    val isStreaming = content.isNotEmpty()
                    _uiState.value = _uiState.value.copy(
                        streamingContent = content,
                        turnState = if (isStreaming) TurnState.STREAMING_TEXT else TurnState.IDLE,
                    )
                }
            }
            launch {
                acc.thinkingContent.collect { content ->
                    _uiState.value = _uiState.value.copy(thinkingContent = content)
                }
            }
            launch {
                acc.messages.collect { accumulated ->
                    val existing = _uiState.value.messages.filter { it.id.startsWith("hist-") }
                    val newMessages = accumulated.map { msg ->
                        when (msg) {
                            is dev.clawseed.sdk.android.AccumulatedMessage.User -> ChatEntry.UserMessage(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                content = msg.content,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.Assistant -> ChatEntry.AssistantMessage(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                content = msg.content,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.ToolCall -> ChatEntry.ToolCall(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                toolCallId = msg.callId,
                                toolName = msg.name,
                                toolArgs = msg.args,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.ToolResult -> ChatEntry.ToolResult(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                toolCallId = msg.callId,
                                toolName = msg.name,
                                toolResult = msg.output,
                                toolSuccess = true,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.Thinking -> ChatEntry.Thinking(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                content = msg.content,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.System -> ChatEntry.SystemMessage(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                content = msg.content,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.Debug -> ChatEntry.DebugInfo(
                                id = msg.id,
                                timestamp = msg.timestamp,
                                messagesJson = msg.messagesJson,
                                estimatedTokens = msg.estimatedTokens,
                            )
                            is dev.clawseed.sdk.android.AccumulatedMessage.Error -> {
                                _uiState.value = _uiState.value.copy(error = msg.message)
                                null
                            }
                        }
                    }.filterNotNull()

                    val all = if (historyLoaded) existing + newMessages else newMessages
                    _uiState.value = _uiState.value.copy(messages = all)
                }
            }
            launch {
                acc.sessionTitle.collect { title ->
                    _uiState.value = _uiState.value.copy(sessionName = title)
                }
            }
        }
    }

    private fun observeConnectionState(session: ClawSeedSession) {
        sessionObservationJob?.cancel()
        sessionObservationJob = viewModelScope.launch {
            launch {
                session.connectionState.collect { state ->
                    val error = if (state == ConnectionState.CONNECTED) null else _uiState.value.error
                    _uiState.value = _uiState.value.copy(connState = state, error = error)
                }
            }
            launch {
                session.sessionInfo.collect { info ->
                    _uiState.value = _uiState.value.copy(
                        sessionName = info?.name ?: _uiState.value.sessionName,
                        currentSessionId = info?.sessionId,
                    )
                }
            }
        }
    }

    fun sendMessage(content: String) {
        if (content.isNotBlank()) {
            accumulator?.addUserMessage(content)
            currentSession?.sendMessage(content, debugEnabled)
        }
    }

    fun abortGeneration() {
        viewModelScope.launch {
            currentSession?.abort()
        }
    }

    fun clearError() {
        _uiState.value = _uiState.value.copy(error = null)
    }

    fun dismissAuthPrompt() {
        _uiState.value = _uiState.value.copy(authPrompt = null)
    }

    fun handleAuthAction() {
        val prompt = _uiState.value.authPrompt ?: return
        val intentStr = prompt.authorizeIntent
        if (intentStr != null) {
            val intent = android.content.Intent(intentStr).apply {
                addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            getApplication<Application>().startActivity(intent)
        }
        _uiState.value = _uiState.value.copy(authPrompt = null)
    }

    private fun observeAuthEvents() {
        authEventJob?.cancel()
        authEventJob = viewModelScope.launch {
            ClawSeedAndroid.externalToolBridge().authEvents.collect { event ->
                _uiState.value = _uiState.value.copy(
                    authPrompt = AuthPrompt(
                        hint = event.resolutionHint ?: "请在 ${event.providerLabel} App 中完成授权",
                        authorizeIntent = event.authorizeIntent,
                    ),
                )
            }
        }
    }

    @Suppress("MissingPermission")
    private fun handleGetLocation(): ToolResult {
        val ctx = getApplication<Application>()

        val hasPermission = ContextCompat.checkSelfPermission(
            ctx, Manifest.permission.ACCESS_FINE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED || ContextCompat.checkSelfPermission(
            ctx, Manifest.permission.ACCESS_COARSE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED

        if (!hasPermission) {
            return ToolResult.Failure("位置权限未授予，请在系统设置中允许ClawSeed访问位置信息")
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
            return ToolResult.Failure("无法获取位置信息，请确保GPS或网络定位已开启")
        }

        val gcj02 = dev.clawseed.demo.CoordinateConverter.wgs84ToGcj02(bestLocation.latitude, bestLocation.longitude)

        val result = buildJsonObject {
            put("latitude", gcj02.latitude)
            put("longitude", gcj02.longitude)
            put("accuracy_meters", bestLocation.accuracy.toDouble())
            put("provider", bestLocation.provider ?: "")
        }

        try {
            val geocoder = Geocoder(ctx, Locale.getDefault())
            @Suppress("DEPRECATION")
            val addresses = geocoder.getFromLocation(bestLocation.latitude, bestLocation.longitude, 1)
            if (!addresses.isNullOrEmpty()) {
                val addr = addresses[0]
                val additional = buildJsonObject {
                    addr.locality?.let { put("city", it) }
                    addr.adminArea?.let { put("province", it) }
                    addr.subLocality?.let { put("district", it) }
                    addr.getAddressLine(0)?.let { put("address", it) }
                }
                // Merge additional fields into result
                val merged = kotlinx.serialization.json.buildJsonObject {
                    result.forEach { (k, v) -> put(k, v) }
                    additional.forEach { (k, v) -> put(k, v) }
                }
                return ToolResult.Success(merged.toString())
            }
        } catch (_: Exception) {
            // Geocoder not available on this device
        }

        return ToolResult.Success(result.toString())
    }
}
