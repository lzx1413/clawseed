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
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import dev.clawseed.demo.scheduled.TaskRepeat
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
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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
    private var registeredSession: ClawSeedSession? = null
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

                if (registeredSession !== session) {
                    registerTools(session)
                    registeredSession = session
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

        session.tools.register(
            name = "scheduled_task",
            description = "管理定时任务。支持查询、创建、删除定时任务。定时任务会在设定的时间自动唤醒设备并执行指定的消息。" +
                "操作类型：list=查询所有任务，add=创建新任务，delete=删除指定任务。" +
                "repeat可选值：once=单次，daily=每天，weekday=工作日。",
            parameters = """{
                "type":"object",
                "properties":{
                    "operation":{"type":"string","enum":["list","add","delete"],"description":"操作类型"},
                    "name":{"type":"string","description":"任务名称（add时必填）"},
                    "message":{"type":"string","description":"到时间后发送给AI执行的消息内容（add时必填）"},
                    "hour":{"type":"integer","description":"执行时间-小时0-23（add时必填）"},
                    "minute":{"type":"integer","description":"执行时间-分钟0-59（add时必填）"},
                    "repeat":{"type":"string","enum":["once","daily","weekday"],"description":"重复模式，默认daily"},
                    "task_id":{"type":"string","description":"任务ID（delete时必填）"}
                },
                "required":["operation"]
            }""",
        ) { args ->
            handleScheduledTask(args)
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

    private suspend fun handleScheduledTask(args: kotlinx.serialization.json.JsonObject): ToolResult {
        val op = args["operation"]?.jsonPrimitive?.content ?: return ToolResult.Failure("缺少 operation 参数")
        val ctx = getApplication<Application>()
        val store = ScheduledTaskStore(ctx)

        when (op) {
            "list" -> {
                val tasks = store.tasksAsList()
                val arr = kotlinx.serialization.json.buildJsonArray {
                    for (task in tasks) {
                        add(kotlinx.serialization.json.buildJsonObject {
                            put("id", task.id)
                            put("name", task.name)
                            put("message", task.message)
                            put("hour", task.hour)
                            put("minute", task.minute)
                            put("repeat", task.repeat.name.lowercase())
                            put("enabled", task.enabled)
                            task.lastRunAt?.let { put("last_run_at", it) }
                            task.lastStatus?.let { put("last_status", it.name.lowercase()) }
                            task.lastError?.let { put("last_error", it) }
                            task.lastResult?.let { put("last_result", it) }
                        })
                    }
                }
                return ToolResult.Success(buildJsonObject { put("tasks", arr) }.toString())
            }
            "add" -> {
                val name = args["name"]?.jsonPrimitive?.content
                    ?: return ToolResult.Failure("缺少 name 参数")
                val message = args["message"]?.jsonPrimitive?.content
                    ?: return ToolResult.Failure("缺少 message 参数")
                val hour = args["hour"]?.jsonPrimitive?.int
                    ?: return ToolResult.Failure("缺少 hour 参数")
                val minute = args["minute"]?.jsonPrimitive?.int
                    ?: return ToolResult.Failure("缺少 minute 参数")
                if (hour !in 0..23) return ToolResult.Failure("hour 必须在 0-23 之间")
                if (minute !in 0..59) return ToolResult.Failure("minute 必须在 0-59 之间")
                val repeatStr = args["repeat"]?.jsonPrimitive?.content ?: "daily"
                val repeat = when (repeatStr) {
                    "once" -> TaskRepeat.ONCE
                    "daily" -> TaskRepeat.DAILY
                    "weekday" -> TaskRepeat.WEEKDAY
                    else -> return ToolResult.Failure("repeat 必须是 once/daily/weekday")
                }
                val currentSessionId = currentSession?.sessionInfo?.value?.sessionId
                val task = ScheduledTask(
                    name = name,
                    message = message,
                    hour = hour,
                    minute = minute,
                    repeat = repeat,
                    sessionId = currentSessionId,
                )
                store.addTask(task)
                ScheduledTaskManager.scheduleAlarm(ctx, task)
                return ToolResult.Success(buildJsonObject {
                    put("id", task.id)
                    put("name", task.name)
                    put("scheduled", "${String.format("%02d:%02d", hour, minute)} ${repeatStr}")
                }.toString())
            }
            "delete" -> {
                val taskId = args["task_id"]?.jsonPrimitive?.content
                    ?: return ToolResult.Failure("缺少 task_id 参数")
                val existing = store.tasksAsList().find { it.id == taskId }
                    ?: return ToolResult.Failure("任务 $taskId 不存在")
                ScheduledTaskManager.cancelAlarm(ctx, taskId)
                store.deleteTask(taskId)
                return ToolResult.Success(buildJsonObject {
                    put("deleted", taskId)
                    put("name", existing.name)
                }.toString())
            }
            else -> return ToolResult.Failure("未知操作: $op，支持 list/add/delete")
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
