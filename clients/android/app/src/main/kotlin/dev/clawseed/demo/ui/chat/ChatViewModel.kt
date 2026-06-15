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
import dev.clawseed.demo.R
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.data.ToolCallInfo
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
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.util.Locale

/**
 * Strip enrichment prefixes that the gateway adds for LLM prompt cache fidelity.
 * These are invisible to the end-user and should not appear in the chat UI:
 * - Timestamp prefix: [YYYY-MM-DD HH:MM:SS TZ]
 * - Memory context block: [Memory context]...\n[/Memory context]\n\n
 */
fun stripEnrichmentPrefixes(content: String): String {
    // Strip timestamp prefix: [YYYY-MM-DD HH:MM:SS TZ]
    var result = content.replace(Regex("^\\[\\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}:\\d{2} [^\\]]+\\]\\s*"), "")
    // Strip memory context prefix: [Memory context]...\n[/Memory context]\n\n
    result = result.replace(Regex("^\\[Memory context\\]\\n.*?\\n\\[/Memory context\\]\\n\\n"), "")
    return result
}

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

/**
 * Per-session state kept alive in the background pool.
 * When the user switches away, the accumulator keeps collecting
 * events so that switching back shows the completed response.
 */
private data class SessionSlot(
    val session: ClawSeedSession,
    val accumulator: ChatAccumulator,
)

    /**
     * Maps accumulated messages to ChatEntry list, merging ToolCall + ToolResult
     * into grouped ToolInvocations entries. Returns entries and collected error messages.
     */
    private fun mapAccumulatedToEntries(
        accumulated: List<dev.clawseed.sdk.android.AccumulatedMessage>,
        stripEnrichment: Boolean = false,
    ): Pair<List<ChatEntry>, List<String>> {
        val errors = mutableListOf<String>()

        // Build a lookup of callId → ToolResult data for merging
        val resultMap = accumulated
            .filterIsInstance<dev.clawseed.sdk.android.AccumulatedMessage.ToolResult>()
            .associateBy { it.callId }

        // Intermediate list: ToolCallInfo for tool entries, ChatEntry for others
        val intermediate = accumulated.mapNotNull { msg ->
            when (msg) {
                is dev.clawseed.sdk.android.AccumulatedMessage.User -> ChatEntry.UserMessage(
                    id = msg.id,
                    timestamp = msg.timestamp,
                    content = if (stripEnrichment) stripEnrichmentPrefixes(msg.content) else msg.content,
                )
                is dev.clawseed.sdk.android.AccumulatedMessage.Assistant -> ChatEntry.AssistantMessage(
                    id = msg.id,
                    timestamp = msg.timestamp,
                    content = msg.content,
                )
                is dev.clawseed.sdk.android.AccumulatedMessage.ToolCall -> {
                    val result = resultMap[msg.callId]
                    ToolCallInfo(
                        toolCallId = msg.callId,
                        toolName = msg.name,
                        toolArgs = msg.args,
                        toolResult = result?.output,
                        toolSuccess = if (result != null) true else null,
                    )
                }
                // ToolResult is merged into ToolCallInfo above; skip standalone entry
                is dev.clawseed.sdk.android.AccumulatedMessage.ToolResult -> null
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
                    errors.add(msg.message)
                    null
                }
            }
        }

        // Group consecutive ToolCallInfo items into ChatEntry.ToolInvocations
        val entries = groupToolCalls(intermediate)
        return Pair(entries, errors)
    }

    /**
     * Scans a mixed list of ChatEntry and ToolCallInfo items, grouping consecutive
     * ToolCallInfo items into ChatEntry.ToolInvocations entries.
     */
    private fun groupToolCalls(items: List<Any>): List<ChatEntry> {
        val result = mutableListOf<ChatEntry>()
        var pendingTools = mutableListOf<ToolCallInfo>()
        var pendingToolIds = mutableListOf<String>()

        for (item in items) {
            if (item is ToolCallInfo) {
                pendingTools.add(item)
            } else {
                if (pendingTools.isNotEmpty()) {
                    result.add(ChatEntry.ToolInvocations(
                        id = "tools-${pendingToolIds.joinToString("-")}",
                        timestamp = pendingTools.first().let { System.currentTimeMillis() },
                        invocations = pendingTools.toList(),
                    ))
                    pendingTools = mutableListOf()
                    pendingToolIds = mutableListOf()
                }
                result.add(item as ChatEntry)
            }
        }
        // Flush remaining tool group
        if (pendingTools.isNotEmpty()) {
            result.add(ChatEntry.ToolInvocations(
                id = "tools-${pendingToolIds.joinToString("-")}",
                timestamp = System.currentTimeMillis(),
                invocations = pendingTools.toList(),
            ))
        }
        return result
    }

class ChatViewModel(application: Application) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    private val localStore = LocalStore(application)
    private var debugEnabled = false
    private var historyLoaded = false

    /** Pool of active session slots. Kept alive across session switches. */
    private val sessionSlots = HashMap<String, SessionSlot>()
    private var registeredSession: ClawSeedSession? = null
    private var currentSessionId: String? = null
    private var currentSession: ClawSeedSession? = null
    private var accumulator: ChatAccumulator? = null
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
        // If already connected to the same session, skip reconnection
        val currentSid = currentSession?.sessionInfo?.value?.sessionId
        if (currentSid != null && currentSid == sessionId
            && currentSession?.connectionState?.value == ConnectionState.CONNECTED
        ) {
            return
        }

        // ── Save the current slot to the pool (don't disconnect or reset) ──
        val oldSid = currentSession?.sessionInfo?.value?.sessionId ?: currentSessionId
        if (oldSid != null && currentSession != null && accumulator != null) {
            sessionSlots[oldSid] = SessionSlot(currentSession!!, accumulator!!)
        }

        // ── Cancel UI observation of the old session ──
        // The accumulator's collection job keeps running — events are still
        // collected in the background so the response is preserved.
        accumulatorObservationJob?.cancel()
        sessionObservationJob?.cancel()
        connectJob?.cancel()
        historyLoaded = false
        currentSessionId = null

        // ── Reset UI state for the new session ──
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

        // ── Check pool for an existing slot ──
        if (sessionId != null && sessionSlots.containsKey(sessionId)) {
            reuseExistingSlot(sessionId)
        } else {
            doConnect(sessionId)
        }
    }

    /**
     * Reuse a session slot from the pool.  The accumulator has been running
     * in the background, so its state reflects any events that occurred
     * while the user was viewing a different session.
     */
    private fun reuseExistingSlot(sessionId: String) {
        val slot = sessionSlots[sessionId]!!
        currentSession = slot.session
        currentSessionId = sessionId
        accumulator = slot.accumulator
        historyLoaded = true // accumulator already has accumulated messages

        // Populate UI from the existing accumulator's current state
        val (existingMessages, errors) = mapAccumulatedToEntries(
            slot.accumulator.messages.value,
            stripEnrichment = true,
        )

        val isStreaming = slot.accumulator.streamingContent.value.isNotEmpty()
        _uiState.value = _uiState.value.copy(
            messages = existingMessages,
            streamingContent = slot.accumulator.streamingContent.value,
            thinkingContent = slot.accumulator.thinkingContent.value,
            turnState = if (isStreaming) TurnState.STREAMING_TEXT else TurnState.IDLE,
            connState = slot.session.connectionState.value,
            sessionName = slot.accumulator.sessionTitle.value ?: slot.session.sessionInfo.value?.name,
            currentSessionId = sessionId,
        )

        // Resume observation
        observeAccumulator(slot.accumulator)
        observeConnectionState(slot.session)
        observeAuthEvents()
    }

    private fun doConnect(sessionId: String?) {
        connectJob?.cancel()
        connectJob = viewModelScope.launch {
            try {
                ClawSeedAndroid.awaitInit()
                val session = sessionManager().connect(sessionId)
                currentSession = session

                val sid = session.sessionInfo.value?.sessionId ?: sessionId
                currentSessionId = sid

                if (registeredSession !== session) {
                    registerTools(session)
                    registeredSession = session
                }

                if (sid != null) {
                    loadHistory(session, sid)
                }

                // Cancel old observation jobs (don't reset old accumulator)
                accumulatorObservationJob?.cancel()
                sessionObservationJob?.cancel()

                // Set up new accumulator
                val acc = ChatAccumulator(session)
                acc.startIn(viewModelScope)
                accumulator = acc

                // Save to pool immediately so it survives future switches
                if (sid != null) {
                    sessionSlots[sid] = SessionSlot(session, acc)
                }

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

        session.tools.register(
            name = "set_alarm",
            description = "在设备上设置闹钟，通过系统时钟应用创建真正的闹钟（会响铃和震动唤醒用户）。适用于用户需要被闹钟叫醒或提醒的场景。" +
                "参数：hour（小时0-23）、minute（分钟0-59）、message（闹钟标签，可选）、repeat_days（重复的星期几1-7对应周一到周日，可选，空表示一次性闹钟）",
            parameters = """{"type":"object","properties":{"hour":{"type":"integer","description":"闹钟小时（0-23）","minimum":0,"maximum":23},"minute":{"type":"integer","description":"闹钟分钟（0-59）","minimum":0,"maximum":59},"message":{"type":"string","description":"闹钟标签/备注信息"},"repeat_days":{"type":"array","description":"重复的星期几（1=周一，2=周二，...7=周日），空数组或null表示一次性闹钟","items":{"type":"integer","minimum":1,"maximum":7}}},"required":["hour","minute"]}""",
        ) { args ->
            handleSetAlarm(args)
        }
    }

    private suspend fun loadHistory(session: ClawSeedSession, sessionId: String) {
        session.gateway.sessionMessages(sessionId)
            .onSuccess { msgs ->
                historyLoaded = true

                // Pass 1: Collect reasoning_content per turn (indexed by user message position)
                val turnThinkingMap = mutableMapOf<Int, String>()
                var currentTurnReasoning = mutableListOf<String>()
                var currentTurnStart = -1

                for ((idx, msg) in msgs.withIndex()) {
                    if (msg.role == "user" && msg.type == "chat") {
                        // Save previous turn's accumulated reasoning
                        if (currentTurnReasoning.isNotEmpty() && currentTurnStart >= 0) {
                            turnThinkingMap[currentTurnStart] = currentTurnReasoning.joinToString("\n\n")
                        }
                        currentTurnStart = idx
                        currentTurnReasoning = mutableListOf()
                    }
                    if (msg.type == "assistant_tool_calls") {
                        val reasoning = msg.data?.jsonObject
                            ?.get("reasoning_content")?.jsonPrimitive?.content
                        if (!reasoning.isNullOrEmpty()) {
                            currentTurnReasoning.add(reasoning)
                        }
                    }
                }
                // Flush last turn's reasoning
                if (currentTurnReasoning.isNotEmpty() && currentTurnStart >= 0) {
                    turnThinkingMap[currentTurnStart] = currentTurnReasoning.joinToString("\n\n")
                }

                // Pass 2: Build intermediate list, inserting Thinking right after UserMessage
                val intermediate = mutableListOf<Any>()

                for ((idx, msg) in msgs.withIndex()) {
                    when (msg.type) {
                        "chat" -> when (msg.role) {
                            "user" -> {
                                intermediate.add(ChatEntry.UserMessage(
                                    id = "hist-$idx",
                                    timestamp = System.currentTimeMillis(),
                                    content = stripEnrichmentPrefixes(msg.content ?: ""),
                                ))
                                // Insert consolidated Thinking right after UserMessage
                                turnThinkingMap[idx]?.let { reasoning ->
                                    intermediate.add(ChatEntry.Thinking(
                                        id = "hist-think-$idx",
                                        timestamp = System.currentTimeMillis(),
                                        content = reasoning,
                                    ))
                                }
                            }
                            "assistant" -> intermediate.add(ChatEntry.AssistantMessage(
                                id = "hist-$idx",
                                timestamp = System.currentTimeMillis(),
                                content = msg.content ?: "",
                            ))
                            else -> {}
                        }
                        "assistant_tool_calls" -> {
                            val data = msg.data?.jsonObject
                            val toolCalls = data?.get("tool_calls")?.jsonArray
                            if (toolCalls != null) {
                                for (tc in toolCalls) {
                                    val tcObj = tc.jsonObject
                                    intermediate.add(ToolCallInfo(
                                        toolCallId = tcObj["id"]?.jsonPrimitive?.content ?: "",
                                        toolName = tcObj["name"]?.jsonPrimitive?.content ?: "",
                                        toolArgs = tcObj["arguments"]?.jsonPrimitive?.content ?: "",
                                    ))
                                }
                            }
                            val text = data?.get("text")?.jsonPrimitive?.content ?: msg.content ?: ""
                            if (text.isNotEmpty()) {
                                intermediate.add(ChatEntry.AssistantMessage(
                                    id = "hist-assist-$idx",
                                    timestamp = System.currentTimeMillis(),
                                    content = text,
                                ))
                            }
                        }
                        "tool_results" -> {
                            val results = msg.data?.jsonArray
                            if (results != null) {
                                for (result in results) {
                                    val resultObj = result.jsonObject
                                    intermediate.add(ToolCallInfo(
                                        toolCallId = resultObj["tool_call_id"]?.jsonPrimitive?.content ?: "",
                                        toolName = resultObj["name"]?.jsonPrimitive?.content ?: "",
                                        toolArgs = "",
                                        toolResult = resultObj["content"]?.jsonPrimitive?.content ?: "",
                                        toolSuccess = true,
                                    ))
                                }
                            }
                        }
                        else -> {}
                    }
                }

                val historyEntries = groupToolCalls(intermediate)
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
                    val (newMessages, errors) = mapAccumulatedToEntries(accumulated)

                    val all = if (historyLoaded) existing + newMessages else newMessages
                    _uiState.value = _uiState.value.copy(
                        messages = all,
                        error = errors.lastOrNull() ?: _uiState.value.error,
                    )
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
                    // Update pool slot sessionId if the gateway assigned a new one
                    val sid = info?.sessionId
                    if (sid != null && currentSessionId != sid) {
                        currentSessionId = sid
                        val slot = sessionSlots[currentSessionId]
                        if (slot != null && slot.session === session) {
                            // Migrate slot entry from old key to real sessionId
                            sessionSlots[sid] = slot
                        } else {
                            sessionSlots[sid] = SessionSlot(session, accumulator ?: ChatAccumulator(session))
                        }
                    }
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

    fun regenerateLastResponse() {
        accumulator?.prepareRegenerate()
        currentSession?.regenerate(debugEnabled)
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
                        hint = event.resolutionHint ?: getApplication<Application>().getString(R.string.chat_auth_hint, event.providerLabel),
                        authorizeIntent = event.authorizeIntent,
                    ),
                )
            }
        }
    }

    private suspend fun handleScheduledTask(args: kotlinx.serialization.json.JsonObject): ToolResult {
        val op = args["operation"]?.jsonPrimitive?.content ?: return ToolResult.Failure("Missing operation parameter")
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
                    ?: return ToolResult.Failure("Missing name parameter")
                val message = args["message"]?.jsonPrimitive?.content
                    ?: return ToolResult.Failure("Missing message parameter")
                val hour = args["hour"]?.jsonPrimitive?.int
                    ?: return ToolResult.Failure("Missing hour parameter")
                val minute = args["minute"]?.jsonPrimitive?.int
                    ?: return ToolResult.Failure("Missing minute parameter")
                if (hour !in 0..23) return ToolResult.Failure("hour must be between 0-23")
                if (minute !in 0..59) return ToolResult.Failure("minute must be between 0-59")
                val repeatStr = args["repeat"]?.jsonPrimitive?.content ?: "daily"
                val repeat = when (repeatStr) {
                    "once" -> TaskRepeat.ONCE
                    "daily" -> TaskRepeat.DAILY
                    "weekday" -> TaskRepeat.WEEKDAY
                    else -> return ToolResult.Failure("repeat must be once/daily/weekday")
                }
                val currentSid = currentSession?.sessionInfo?.value?.sessionId
                val task = ScheduledTask(
                    name = name,
                    message = message,
                    hour = hour,
                    minute = minute,
                    repeat = repeat,
                    sessionId = currentSid,
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
                    ?: return ToolResult.Failure("Missing task_id parameter")
                val existing = store.tasksAsList().find { it.id == taskId }
                    ?: return ToolResult.Failure("Task $taskId not found")
                ScheduledTaskManager.cancelAlarm(ctx, taskId)
                store.deleteTask(taskId)
                return ToolResult.Success(buildJsonObject {
                    put("deleted", taskId)
                    put("name", existing.name)
                }.toString())
            }
            else -> return ToolResult.Failure("Unknown operation: $op, supported: list/add/delete")
        }
    }

    private suspend fun handleSetAlarm(args: kotlinx.serialization.json.JsonObject): ToolResult {
        val ctx = getApplication<Application>()
        val hour = args["hour"]?.jsonPrimitive?.intOrNull
        val minute = args["minute"]?.jsonPrimitive?.intOrNull

        if (hour == null || minute == null) {
            return ToolResult.Failure("Missing required parameters: hour and minute")
        }
        if (hour < 0 || hour > 23 || minute < 0 || minute > 59) {
            return ToolResult.Failure("Invalid parameter range: hour should be 0-23, minute should be 0-59")
        }

        val message = args["message"]?.jsonPrimitive?.content ?: ""
        val repeatDays = args["repeat_days"]?.jsonArray?.mapNotNull { it.jsonPrimitive.intOrNull }

        val repeat = if (repeatDays == null || repeatDays.isEmpty()) {
            TaskRepeat.ONCE
        } else if (repeatDays.size == 5 && repeatDays.containsAll(listOf(1, 2, 3, 4, 5))) {
            TaskRepeat.WEEKDAY
        } else {
            TaskRepeat.DAILY
        }

        val taskName = if (message.isNotEmpty()) "Alarm: $message" else "Alarm ${String.format("%02d:%02d", hour, minute)}"
        val alarmMessage = if (message.isNotEmpty()) message else "Alarm fired"

        val store = ScheduledTaskStore(ctx)
        val task = ScheduledTask(
            name = taskName,
            message = alarmMessage,
            hour = hour,
            minute = minute,
            repeat = repeat,
            enabled = true,
            isAlarm = true,
        )
        store.addTask(task)
        ScheduledTaskManager.scheduleAlarm(ctx, task)

        return ToolResult.Success(buildJsonObject {
            put("id", task.id)
            put("name", taskName)
            put("hour", hour)
            put("minute", minute)
            put("repeat", repeat.name.lowercase())
            put("is_alarm", true)
        }.toString())
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
            return ToolResult.Failure("Location permission not granted, please allow ClawSeed access in system settings")
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
            return ToolResult.Failure("Unable to get location, please ensure GPS or network location is enabled")
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
