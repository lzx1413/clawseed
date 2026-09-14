package dev.clawseed.demo.ui.chat

import android.Manifest
import android.app.PendingIntent
import android.app.Application
import android.content.Context
import android.content.pm.PackageManager
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import dev.clawseed.demo.R
import dev.clawseed.demo.BackgroundJobNotifier
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.data.ToolCallInfo
import dev.clawseed.demo.data.TurnState
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.ui.chat.markdown.toSpeakableText
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import dev.clawseed.demo.scheduled.TaskRepeat
import dev.clawseed.demo.scheduled.AlarmSchedule
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.android.ChatAccumulator
import dev.clawseed.sdk.android.cetp.AuthRequiredEvent
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.model.ToolPresentation
import dev.clawseed.sdk.core.model.parseToolPresentation
import dev.clawseed.sdk.core.tool.ToolResult
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Job
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.async
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.JsonElement
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
    val providerPackageName: String,
    val hint: String,
    val authorizeIntent: String?,
    val resolution: PendingIntent?,
    val requestId: String?,
    val errorCode: String,
)

data class ChatUiState(
    val showDebugInfo: Boolean = false,
    val imageAttachmentsSupported: Boolean = false,
    val fileAttachmentsSupported: Boolean = false,
    val messages: List<ChatEntry> = emptyList(),
    val streamingContent: String = "",
    val thinkingContent: String = "",
    val turnState: TurnState = TurnState.IDLE,
    val isGenerating: Boolean = false,
    val connState: ConnectionState = ConnectionState.DISCONNECTED,
    val sessionName: String? = null,
    val currentSessionId: String? = null,
    val error: String? = null,
    val authPrompt: AuthPrompt? = null,
    val pendingQuestion: ChatEvent.AskUserRequested? = null,
    val questionSubmitting: Boolean = false,
    val speechOutputEnabled: Boolean = false,
    val isSpeaking: Boolean = false,
    val speakingMessageId: String? = null,
    /** Persona bound to the active session (from session_start echo). Null = default. */
    val currentPersona: String? = null,
    val personaVisuals: Map<String, PersonaInfo> = emptyMap(),
)

/**
 * Per-session state kept alive in the background pool.
 * When the user switches away, the accumulator keeps collecting
 * events so that switching back shows the completed response.
 */
internal data class SessionSlot(
    val session: ClawSeedSession,
    val accumulator: ChatAccumulator,
    var history: List<ChatEntry> = emptyList(),
    var lifetimeJob: Job? = null,
    var pendingQuestion: ChatEvent.AskUserRequested? = null,
    var questionSubmitting: Boolean = false,
) {
    fun close() {
        lifetimeJob?.cancel()
        accumulator.stop()
    }

    fun messages(): List<ChatEntry> = history + mapAccumulatedToEntries(accumulator.messages.value).first

    fun sendMessage(content: String, debug: Boolean = false, expectedSessionId: String? = null, attachments: List<dev.clawseed.sdk.core.model.ImageAttachment> = emptyList(), files: List<dev.clawseed.sdk.core.model.FileAttachment> = emptyList()): Boolean {
        if ((content.isBlank() && attachments.isEmpty() && files.isEmpty()) || accumulator.isGenerating.value || session.connectionState.value != ConnectionState.CONNECTED) return false
        if (expectedSessionId != null && session.sessionInfo.value?.sessionId != expectedSessionId) return false
        accumulator.addUserMessage(content, attachments, files)
        return runCatching { session.sendMessage(content, debug, attachments, files) }
            .onFailure { accumulator.failTurn(it.message ?: "Failed to send message") }
            .isSuccess
    }

    fun regenerate(debug: Boolean = false) {
        if (accumulator.isGenerating.value || session.connectionState.value != ConnectionState.CONNECTED) return
        prepareRegenerate()
        runCatching { session.regenerate(debug) }
            .onFailure { accumulator.failTurn(it.message ?: "Failed to regenerate response") }
    }

    fun prepareRegenerate() {
        if (accumulator.messages.value.none { it is dev.clawseed.sdk.android.AccumulatedMessage.User }) {
            val lastUser = history.indexOfLast { it is ChatEntry.UserMessage }
            if (lastUser >= 0) history = history.take(lastUser + 1)
        }
        accumulator.prepareRegenerate()
    }
}

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

        // Presentations belong to the completed assistant turn, not to the
        // diagnostic tool-call row. Collect them as tool results arrive and
        // attach them to the next assistant message flushed for this turn.
        val pendingPresentationBlocks = mutableListOf<dev.clawseed.sdk.core.model.ContentBlock>()
        val intermediate = mutableListOf<Any>()
        for (msg in accumulated) {
            when (msg) {
                is dev.clawseed.sdk.android.AccumulatedMessage.User -> {
                    // A user boundary must never inherit media from an
                    // interrupted or failed preceding turn.
                    pendingPresentationBlocks.clear()
                    intermediate.add(ChatEntry.UserMessage(
                        id = msg.id,
                        timestamp = msg.timestamp,
                        content = if (stripEnrichment) stripEnrichmentPrefixes(msg.content) else msg.content,
                        attachments = msg.attachments,
                        files = msg.files,
                    ))
                }
                is dev.clawseed.sdk.android.AccumulatedMessage.Assistant -> {
                    val presentation = pendingPresentationBlocks
                        .takeIf { it.isNotEmpty() }
                        ?.let { ToolPresentation(version = 1, blocks = it.toList()) }
                    pendingPresentationBlocks.clear()
                    intermediate.add(ChatEntry.AssistantMessage(
                        id = msg.id,
                        timestamp = msg.timestamp,
                        content = msg.content,
                        presentation = presentation,
                        metrics = msg.metrics,
                    ))
                }
                is dev.clawseed.sdk.android.AccumulatedMessage.ToolCall -> {
                    val result = resultMap[msg.callId]
                    intermediate.add(ToolCallInfo(
                        toolCallId = msg.callId,
                        toolName = msg.name,
                        toolArgs = msg.args,
                        toolResult = result?.output,
                        toolSuccess = if (result != null) true else null,
                    ))
                }
                is dev.clawseed.sdk.android.AccumulatedMessage.ToolResult -> {
                    msg.presentation?.blocks?.let(pendingPresentationBlocks::addAll)
                }
                is dev.clawseed.sdk.android.AccumulatedMessage.Thinking -> intermediate.add(ChatEntry.Thinking(
                    id = msg.id,
                    timestamp = msg.timestamp,
                    content = msg.content,
                ))
                is dev.clawseed.sdk.android.AccumulatedMessage.System -> intermediate.add(ChatEntry.SystemMessage(
                    id = msg.id,
                    timestamp = msg.timestamp,
                    content = msg.content,
                ))
                is dev.clawseed.sdk.android.AccumulatedMessage.Debug -> intermediate.add(ChatEntry.DebugInfo(
                    id = msg.id,
                    timestamp = msg.timestamp,
                    messagesJson = msg.messagesJson,
                    estimatedTokens = msg.estimatedTokens,
                    toolsJson = msg.toolsJson,
                    estimatedToolTokens = msg.estimatedToolTokens,
                ))
                is dev.clawseed.sdk.android.AccumulatedMessage.Error -> {
                    errors.add(msg.message)
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
        var pendingStartIndex = -1

        fun flushPendingTools() {
            if (pendingTools.isEmpty()) return
            result.add(ChatEntry.ToolInvocations(
                id = toolGroupId(pendingStartIndex, pendingTools),
                timestamp = System.currentTimeMillis(),
                invocations = pendingTools.toList(),
            ))
            pendingTools = mutableListOf()
            pendingStartIndex = -1
        }

        for ((index, item) in items.withIndex()) {
            if (item is ToolCallInfo) {
                if (pendingTools.isEmpty()) {
                    pendingStartIndex = index
                }
                pendingTools.add(item)
            } else {
                flushPendingTools()
                result.add(item as ChatEntry)
            }
        }
        flushPendingTools()
        return result
    }

private fun toolGroupId(startIndex: Int, tools: List<ToolCallInfo>): String {
    val suffix = tools
        .mapIndexed { index, tool ->
            tool.toolCallId
                .takeIf { it.isNotBlank() }
                ?: "${tool.toolName.ifBlank { "tool" }}-$index"
        }
        .joinToString("-") { it.hashCode().toUInt().toString(16) }
    return "tools-${startIndex.coerceAtLeast(0)}-$suffix"
}

class ChatViewModel(application: Application, private val savedStateHandle: SavedStateHandle) : AndroidViewModel(application) {

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()
    private val draftStore = ChatDrafts(savedStateHandle)
    val drafts = draftStore.drafts

    fun updateDraft(key: String, text: String) {
        draftStore.update(key, text)
    }

    private val fileStore = ChatFileAttachments(application)
    internal val fileDrafts = fileStore.entries
    internal val fileDraftsReady = fileStore.ready

    internal fun addFiles(target: ImageDraftTarget, uris: List<android.net.Uri>) {
        if (imageDraftTarget()?.key != target.key || !imageOperations.add(target.key)) return
        viewModelScope.launch {
            try {
                check(currentSlot?.session?.sessionInfo?.value?.fileAttachmentsSupported == true) { "当前 Gateway 不支持文件附件，请升级 Gateway" }
                check(fileDrafts.value[target.key].orEmpty().count { !it.sent } + uris.size <= 4) { "每条消息最多 4 个文件" }
                for (uri in uris) fileStore.import(target.key, uri)
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                _uiState.value = _uiState.value.copy(error = error.message)
            } finally { imageOperations.remove(target.key) }
        }
    }

    internal fun changeFile(target: ImageDraftTarget, id: String, retry: Boolean) {
        if (!imageOperations.add(target.key)) return
        viewModelScope.launch {
            try { if (retry) fileStore.retry(target.key, id) else fileStore.remove(target.key, id) }
            catch (error: Exception) {
                if (error is CancellationException) throw error
                _uiState.value = _uiState.value.copy(error = error.message)
            } finally { imageOperations.remove(target.key) }
        }
    }

    private val sharedInbox = dev.clawseed.demo.sharing.SharedInbox(application)
    private val checkedShares = mutableSetOf<String>()

    internal fun importSharedDraft(target: ImageDraftTarget) {
        if (target.key in checkedShares || !imageOperations.add(target.key)) return
        viewModelScope.launch {
            try {
                val received = sharedInbox.load(target.sessionId)
                if (received == null || received.imported) {
                    checkedShares += target.key
                    return@launch
                }
                val bundle = sharedInbox.bind(target.sessionId, target.key)
                check(bundle.items.none { it.image } || canPickImages()) { "当前网关不支持图片，分享副本已保存，请切换配置后重试" }
                check(bundle.items.none { !it.image } || currentSlot?.session?.sessionInfo?.value?.fileAttachmentsSupported == true) { "当前网关不支持文件，分享副本已保存，请升级后重试" }
                val staged = mutableListOf<ChatImageDraft>()
                for (item in bundle.items) {
                    val uri = android.net.Uri.fromFile(sharedInbox.file(bundle, item))
                    if (item.image) {
                        val existing = imageDrafts.value[target.key].orEmpty().find { it.id == item.id }
                        if (existing != null) {
                            if (existing.attachment == null) staged += existing
                        } else {
                            check(imageDrafts.value[target.key].orEmpty().size < 4) { "每条消息最多 4 张图片" }
                            val draft = imageDraftStore.stageImage(uri, item.id)
                            imageDraftStore.transform(target.key) { it + draft }
                            staged += draft
                        }
                    } else {
                        val id = "file_" + item.id.replace("-", "")
                        if (fileDrafts.value[target.key].orEmpty().any { it.id == id } && !fileStore.hasOriginal(id)) fileStore.retry(target.key, id)
                        else fileStore.import(target.key, uri, id)
                        check(fileStore.hasOriginal(id)) { "文件副本保存失败，请释放存储空间后重新打开会话" }
                    }
                }
                if (bundle.text.isNotBlank() && imageDraftStore.savedText(target.key).isBlank() && drafts.value[target.sessionId].isNullOrBlank()) {
                    imageDraftStore.saveText(target.key, bundle.text)
                    draftStore.update(target.sessionId, bundle.text)
                }
                sharedInbox.complete(bundle.id)
                checkedShares += target.key
                if (bundle.warnings.isNotEmpty() && imageDraftTarget()?.key == target.key) {
                    _uiState.value = _uiState.value.copy(error = bundle.warnings.joinToString("\n"))
                }
                for (draft in staged) uploadDraft(target, draft)
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                if (imageDraftTarget()?.key == target.key) _uiState.value = _uiState.value.copy(error = error.message ?: "分享导入失败，副本已保留，请重新打开会话重试")
            } finally { imageOperations.remove(target.key) }
        }
    }

    private val imageDraftStore = ChatImageDrafts(application)
    internal val imageDrafts = imageDraftStore.drafts
    internal val imageDraftsReady = imageDraftStore.ready
    private val imageOperations = mutableSetOf<String>()

    internal fun canPickImages(): Boolean =
        currentSlot?.session?.sessionInfo?.value?.imageAttachmentsSupported == true

    internal fun imageDraftTarget(): ImageDraftTarget? {
        val session = currentSlot?.session ?: return null
        val id = session.sessionInfo.value?.sessionId ?: return null
        return ImageDraftTarget(ChatImageDrafts.key(session.gateway, id), id, session.gateway)
    }

    internal fun imageDraftText(key: String): String =
        if (imageDrafts.value[key].orEmpty().any { it.awaitingReply } || fileDrafts.value[key].orEmpty().any { it.awaitingReply }) ""
        else imageDraftStore.savedText(key)

    internal fun imageDraftFile(id: String) = imageDraftStore.file(id)

    internal fun removeImage(target: ImageDraftTarget, id: String) {
        if (!imageOperations.add(target.key)) return
        viewModelScope.launch {
            try { imageDraftStore.remove(target.key, id) }
            catch (error: Exception) {
                if (error is CancellationException) throw error
                _uiState.value = _uiState.value.copy(error = error.message)
            } finally { imageOperations.remove(target.key) }
        }
    }

    internal fun addImages(target: ImageDraftTarget, uris: List<android.net.Uri>, onComplete: () -> Unit = {}) {
        if (!canPickImages() || imageDraftTarget()?.key != target.key || !imageOperations.add(target.key)) {
            onComplete()
            return
        }
        viewModelScope.launch {
            try {
                check(imageDrafts.value[target.key].orEmpty().size + uris.size <= 4) { "每条消息最多选择 4 张图片" }
                val staged = mutableListOf<ChatImageDraft>()
                for (uri in uris) {
                    try {
                        val image = imageDraftStore.stageImage(uri)
                        imageDraftStore.transform(target.key) { it + image }
                        staged += image
                    } catch (error: Exception) {
                        if (error is CancellationException) throw error
                        _uiState.value = _uiState.value.copy(error = error.message ?: "图片读取失败")
                    }
                }
                // Publish every local preview before processing or uploading any image.
                for (image in staged) uploadDraft(target, image)
            } catch (error: Exception) {
                if (error is kotlinx.coroutines.CancellationException) throw error
                _uiState.value = _uiState.value.copy(error = error.message ?: "图片处理失败")
            } finally { imageOperations.remove(target.key); onComplete() }
        }
    }

    internal fun retryImage(target: ImageDraftTarget, id: String) {
        val image = imageDrafts.value[target.key].orEmpty().find { it.id == id } ?: return
        if (image.preparing || image.uploading || !imageOperations.add(target.key)) return
        viewModelScope.launch {
            try { uploadDraft(target, image) }
            catch (error: Exception) {
                if (error is CancellationException) throw error
                _uiState.value = _uiState.value.copy(error = error.message)
            } finally { imageOperations.remove(target.key) }
        }
    }

    private suspend fun uploadDraft(target: ImageDraftTarget, image: ChatImageDraft) {
        var prepared = image
        try {
            if (imageDraftStore.needsPreparation(image.id)) {
                imageDraftStore.replace(target.key, image.copy(preparing = true, error = null))
                prepared = imageDraftStore.prepareImage(image)
            }
            imageDraftStore.replace(target.key, prepared.copy(preparing = false, uploading = true, error = null))
            val bytes = withContext(Dispatchers.IO) { imageDraftStore.file(image.id).readBytes() }
            val attachment = target.gateway.uploadImage(target.sessionId, bytes).getOrThrow()
            imageDraftStore.replace(target.key, prepared.copy(attachment = attachment, preparing = false, uploading = false, error = null))
        } catch (error: Exception) {
            if (error is kotlinx.coroutines.CancellationException) throw error
            imageDraftStore.replace(target.key, prepared.copy(preparing = false, uploading = false, error = error.message ?: "图片处理或上传失败"))
        }
    }

    internal fun sendImageDraft(content: String, target: ImageDraftTarget): Boolean {
        val slot = currentSlot ?: return false
        if (imageDraftTarget()?.key != target.key || target.key in imageOperations) return false
        val images = imageDrafts.value[target.key].orEmpty().filterNot { it.awaitingReply }
        val fileDrafts = fileDrafts.value[target.key].orEmpty().filter { !it.sent && !it.awaitingReply }
        if (images.isEmpty() && fileDrafts.isEmpty()) return sendMessage(content, target.sessionId)
        if (fileDrafts.any { it.status != "ready" }) return false
        if (images.any { it.preparing || it.uploading || it.attachment == null || it.error != null }) return false
        if (!imageOperations.add(target.key)) return false
        viewModelScope.launch {
            try {
                // Preserve crash recovery before sending, without blocking keyboard dismissal.
                imageDraftStore.saveText(target.key, content)
                if (currentSlot !== slot || imageDraftTarget()?.key != target.key) return@launch
                val files = fileStore.metadata(target.key)
                fileStore.markSending(target.key, files.map { it.id }.toSet())
                kotlinx.coroutines.coroutineScope {
                    // Subscribe before sending so a very fast response cannot be missed.
                    val response = async(start = kotlinx.coroutines.CoroutineStart.UNDISPATCHED) {
                        slot.session.events.first {
                            it is dev.clawseed.sdk.core.model.ChatEvent.Done ||
                                it is dev.clawseed.sdk.core.model.ChatEvent.Error ||
                                it is dev.clawseed.sdk.core.model.ChatEvent.Aborted
                        }
                    }
                    val disconnected = async(start = kotlinx.coroutines.CoroutineStart.UNDISPATCHED) {
                        slot.session.connectionState.first { it == ConnectionState.DISCONNECTED }
                        dev.clawseed.sdk.core.model.ChatEvent.Error(
                            getApplication<Application>().getString(R.string.chat_connection_interrupted),
                        )
                    }
                    if (!slot.sendMessage(content, debugEnabled, target.sessionId, images.mapNotNull { it.attachment }, files)) {
                        response.cancel()
                        disconnected.cancel()
                        fileStore.finish(target.key, files.map { it.id }.toSet(), false)
                        return@coroutineScope
                    }
                    val ids = images.map { it.id }.toSet()
                    imageDraftStore.transform(target.key) { current ->
                        current.map { if (it.id in ids) it.copy(awaitingReply = true) else it }
                    }
                    if (drafts.value[target.sessionId] == content) draftStore.update(target.sessionId, "")
                    val event = kotlinx.coroutines.selects.select<dev.clawseed.sdk.core.model.ChatEvent> {
                        response.onAwait { it }
                        disconnected.onAwait { it }
                    }
                    response.cancel()
                    disconnected.cancel()
                    fileStore.finish(target.key, files.map { it.id }.toSet(), event is dev.clawseed.sdk.core.model.ChatEvent.Done)
                    when (event) {
                        is dev.clawseed.sdk.core.model.ChatEvent.Done -> imageDraftStore.removeAll(target.key, ids)
                        else -> imageDraftStore.transform(target.key) { current ->
                            current.map {
                                if (it.id in ids) it.copy(
                                    awaitingReply = false,
                                    error = (event as? dev.clawseed.sdk.core.model.ChatEvent.Error)?.message,
                                ) else it
                            }
                        }
                    }
                }
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                fileStore.finish(target.key, fileDrafts.map { it.id }.toSet(), false)
                _uiState.value = _uiState.value.copy(error = error.message ?: "附件发送失败")
            } finally {
                imageOperations.remove(target.key)
            }
        }
        return true
    }

    private val attachmentImageCache = AttachmentImageCache()

    internal suspend fun readImage(sessionId: String, id: String): Result<ByteArray> {
        val session = currentSlot?.session ?: return Result.failure(IllegalStateException("会话未连接"))
        if (session.sessionInfo.value?.sessionId != sessionId) return Result.failure(IllegalStateException("会话已切换"))
        return attachmentImageCache.load("${ChatImageDrafts.key(session.gateway, sessionId)}:$id") {
            session.gateway.readImage(sessionId, id)
        }
    }

    private val localStore = LocalStore(application)
    private var debugEnabled = false
    private var currentSlot: SessionSlot? = null
    private var migrateNewDraft = false
    private val sessionSwitchVersions = SessionSwitchVersionGate()

    /** Speech output engine. Lives for the ViewModel lifetime; released in onCleared. */
    val tts = TtsController(application)

    // ── Speech playback state ──
    // We speak the AUTHORITATIVE finalized assistant message (not the provisional streaming
    // draft, which the gateway discards during tool-use turns). ttsLastSpokenMsgId dedupes so a
    // message is spoken once even if the messages flow re-emits; it's also used as a baseline
    // when switching into a session so pre-existing messages aren't spoken on arrival.
    private var ttsLastSpokenMsgId: String? = null

    /** Pool of active session slots. Kept alive across session switches. */
    private val sessionSlots = HashMap<String, SessionSlot>()
    private var registeredSession: ClawSeedSession? = null
    private var currentSessionId: String? = null
    private var currentSession: ClawSeedSession? = null
    private var accumulator: ChatAccumulator? = null
    private var connectJob: Job? = null
    private var abortJob: Job? = null
    private var abortingAccumulator: ChatAccumulator? = null
    private var accumulatorObservationJob: Job? = null
    private var sessionObservationJob: Job? = null
    private var authEventJob: Job? = null
    private var activeProviderRequestId: String? = null
    private val queuedAuthPrompts = ArrayDeque<AuthPrompt>()

    init {
        viewModelScope.launch {
            runCatching { fileStore.load() }
                .onFailure { _uiState.value = _uiState.value.copy(error = it.message) }
            runCatching { imageDraftStore.load() }
                .onFailure { _uiState.value = _uiState.value.copy(error = it.message) }
        }
        viewModelScope.launch {
            localStore.showDebugInfo.collect {
                debugEnabled = it
                _uiState.value = _uiState.value.copy(showDebugInfo = it)
            }
        }
        viewModelScope.launch {
            localStore.speechOutputEnabled.collect { enabled ->
                // Flipping speech off should immediately stop any in-progress playback.
                if (!enabled) {
                    tts.stop()
                    ttsLastSpokenMsgId = null
                }
                _uiState.value = _uiState.value.copy(speechOutputEnabled = enabled)
            }
        }
        viewModelScope.launch {
            tts.isSpeaking.collect { speaking ->
                _uiState.value = _uiState.value.copy(isSpeaking = speaking)
            }
        }
        viewModelScope.launch {
            tts.speakingMessageId.collect { id ->
                _uiState.value = _uiState.value.copy(speakingMessageId = id)
            }
        }
        viewModelScope.launch {
            ClawSeedAndroid.awaitInit()
            sessionManager().pooledSessions.collect { pooled ->
                val iterator = sessionSlots.iterator()
                while (iterator.hasNext()) {
                    val (id, slot) = iterator.next()
                    if (pooled[id] !== slot.session) {
                        slot.close()
                        iterator.remove()
                    }
                }
            }
        }
        refreshPersonaVisuals()
    }

    override fun onCleared() {
        activeProviderRequestId?.let {
            ClawSeedAndroid.externalToolBridge().cancelPendingAction(it)
        }
        queuedAuthPrompts.mapNotNull { it.requestId }.forEach {
            ClawSeedAndroid.externalToolBridge().cancelPendingAction(it)
        }
        sessionSlots.values.forEach { it.close() }
        sessionSlots.clear()
        tts.shutdown()
        super.onCleared()
    }

    /**
     * Speak a finalized assistant message in full once it lands. Called from the messages
     * collector when a new authoritative AssistantMessage arrives. We wait for the finalized
     * message (not the provisional streaming draft) so playback always matches what's rendered —
     * tool-use turns discard the draft via ChunkReset and only the authoritative text is spoken.
     */
    private fun speakAssistantMessage(msg: dev.clawseed.sdk.android.AccumulatedMessage.Assistant) {
        ttsLastSpokenMsgId = msg.id
        if (!_uiState.value.speechOutputEnabled) return
        val chunk = cleanForSpeech(msg.content)
        if (chunk.isNotEmpty()) tts.speak(chunk, msg.id)
    }

    /** Light markdown cleanup so symbols like `**`, `#`, backticks aren't read aloud. */
    private fun cleanForSpeech(s: String): String =
        s.replace(Regex("[*_#>~]"), "")
            .replace("`", "")
            .replace(Regex("\\[([^]]+)]\\([^)]+\\)"), "$1")  // links -> label
            .trim()

    fun toggleSpeechOutput() {
        viewModelScope.launch {
            localStore.setSpeechOutputEnabled(!_uiState.value.speechOutputEnabled)
        }
    }

    fun speakMessage(content: String, messageId: String) {
        tts.speak(content.toSpeakableText(), messageId)
    }

    fun stopSpeech() {
        tts.stop()
    }

    private fun sessionManager(): dev.clawseed.sdk.android.SessionManager {
        return ClawSeedAndroid.sessionManager()
    }

    fun refreshPersonaVisuals() {
        viewModelScope.launch {
            runCatching {
                ClawSeedAndroid.awaitInit()
                ClawSeedAndroid.gatewayClient().personas().getOrThrow()
                    .filter { it.isPersona }
                    .associateBy { it.name }
            }.onSuccess { visuals ->
                _uiState.value = _uiState.value.copy(personaVisuals = visuals)
            }
        }
    }

    fun switchToSession(
        sessionId: String?,
        persona: String? = null,
        requestVersion: Int? = null,
    ) {
        if (requestVersion != null && !sessionSwitchVersions.tryAcquire(requestVersion)) {
            return
        }
        refreshPersonaVisuals()
        // If already connected to the same session, skip reconnection
        val currentSid = currentSession?.sessionInfo?.value?.sessionId
        if (currentSid != null && currentSid == sessionId
            && currentSession?.connectionState?.value == ConnectionState.CONNECTED
            && currentSlot != null
        ) {
            return
        }

        // ── Save the current slot to the pool (don't disconnect or reset) ──
        val oldSid = currentSession?.sessionInfo?.value?.sessionId ?: currentSessionId
        if (oldSid != null && currentSession != null && accumulator != null) {
            currentSlot?.let { sessionSlots[oldSid] = it }
        }

        // ── Cancel UI observation of the old session ──
        // The accumulator's collection job keeps running — events are still
        // collected in the background so the response is preserved.
        accumulatorObservationJob?.cancel()
        sessionObservationJob?.cancel()
        connectJob?.cancel()
        currentSlot = null
        migrateNewDraft = sessionId == null
        currentSession = null
        accumulator = null
        currentSessionId = null

        // ── Reset UI state for the new session ──
        _uiState.value = _uiState.value.copy(
            messages = emptyList(),
            streamingContent = "",
            thinkingContent = "",
            turnState = TurnState.IDLE,
            isGenerating = false,
            connState = ConnectionState.DISCONNECTED,
            sessionName = null,
            currentSessionId = null,
            currentPersona = null,
            imageAttachmentsSupported = false,
            fileAttachmentsSupported = false,
            pendingQuestion = null,
            questionSubmitting = false,
            error = null,
        )

        // The SDK owns connection reuse and LRU access order, including cached UI slots.
        doConnect(sessionId, persona)
    }

    /**
     * Start a brand-new session bound to [persona] (null = default global agent).
     * Persona is write-once: it only takes effect on a fresh session; the
     * gateway stores the binding and echoes it back in session_start.
     */
    fun startNewSession(persona: String?) {
        switchToSession(null, persona)
    }

    /**
     * Reuse a session slot from the pool.  The accumulator has been running
     * in the background, so its state reflects any events that occurred
     * while the user was viewing a different session.
     */
    private fun reuseExistingSlot(sessionId: String) {
        val slot = sessionSlots[sessionId]!!
        currentSession = slot.session
        currentSlot = slot
        currentSessionId = sessionId
        accumulator = slot.accumulator

        // Populate UI from the existing accumulator's current state
        val existingMessages = slot.messages()

        val isStreaming = slot.accumulator.streamingContent.value.isNotEmpty()
        _uiState.value = _uiState.value.copy(
            messages = existingMessages,
            streamingContent = slot.accumulator.streamingContent.value,
            thinkingContent = slot.accumulator.thinkingContent.value,
            turnState = if (isStreaming) TurnState.STREAMING_TEXT else TurnState.IDLE,
            isGenerating = slot.accumulator.isGenerating.value,
            error = slot.accumulator.error.value,
            connState = slot.session.connectionState.value,
            sessionName = slot.accumulator.sessionTitle.value ?: slot.session.sessionInfo.value?.name,
            currentSessionId = sessionId,
            currentPersona = slot.session.sessionInfo.value?.persona,
            imageAttachmentsSupported = slot.session.sessionInfo.value?.imageAttachmentsSupported == true,
            fileAttachmentsSupported = slot.session.sessionInfo.value?.fileAttachmentsSupported == true,
            pendingQuestion = slot.pendingQuestion,
            questionSubmitting = slot.questionSubmitting,
        )

        // Resume observation
        observeAccumulator(slot)
        observeConnectionState(slot.session)
        observeAuthEvents()
    }

    private fun doConnect(sessionId: String?, persona: String? = null) {
        connectJob?.cancel()
        connectJob = viewModelScope.launch {
            try {
                ClawSeedAndroid.awaitInit()
                val session = sessionManager().connect(sessionId, persona)
                currentSession = session

                val sid = session.sessionInfo.value?.sessionId ?: sessionId
                currentSessionId = sid
                if (sid != null && sessionSlots[sid]?.session === session) {
                    reuseExistingSlot(sid)
                    return@launch
                }

                if (registeredSession !== session) {
                    registerTools(session)
                    registeredSession = session
                }

                val history = if (sessionId != null) loadHistory(session, sessionId) else emptyList()

                // Cancel old observation jobs (don't reset old accumulator)
                accumulatorObservationJob?.cancel()
                sessionObservationJob?.cancel()

                // Set up new accumulator
                val acc = ChatAccumulator(session)
                acc.startIn(viewModelScope)
                accumulator = acc
                val slot = SessionSlot(
                    session = session,
                    accumulator = acc,
                    history = history,
                    pendingQuestion = session.pendingQuestion.value,
                )
                currentSlot = slot

                // Save to pool immediately so it survives future switches
                if (sid != null) {
                    sessionSlots.put(sid, slot)?.close()
                    slot.lifetimeJob = viewModelScope.launch {
                        launch {
                            acc.isGenerating.collect { sessionManager().setSessionGenerating(sid, it) }
                        }
                        launch {
                            session.connectionState.collect { state ->
                                if (state == ConnectionState.DISCONNECTED && acc.isGenerating.value) {
                                    acc.failTurn(getApplication<Application>().getString(R.string.chat_connection_interrupted))
                                }
                            }
                        }
                        launch {
                            session.pendingQuestion.collect { question ->
                                slot.pendingQuestion = question
                                if (currentSlot === slot) _uiState.value = _uiState.value.copy(
                                    pendingQuestion = question,
                                    questionSubmitting = if (question == null) false else slot.questionSubmitting,
                                )
                            }
                        }
                        launch {
                            session.events.collect { event ->
                                when (event) {
                                    is ChatEvent.BackgroundJobCompleted -> BackgroundJobNotifier.notifyIfBackground(
                                        getApplication<Application>(), event, sid,
                                    )
                                    is ChatEvent.AskUserRequested -> {
                                        slot.pendingQuestion = event
                                        slot.questionSubmitting = false
                                        if (currentSlot === slot) _uiState.value = _uiState.value.copy(
                                            pendingQuestion = event,
                                            questionSubmitting = false,
                                        )
                                    }
                                    is ChatEvent.AskUserAcknowledged -> {
                                        if (slot.pendingQuestion?.requestId == event.requestId) {
                                            if (event.status == "accepted" || event.status == "not_pending") {
                                                slot.pendingQuestion = null
                                                slot.questionSubmitting = false
                                                if (currentSlot === slot) _uiState.value = _uiState.value.copy(
                                                    pendingQuestion = null,
                                                    questionSubmitting = false,
                                                )
                                            } else {
                                                slot.questionSubmitting = false
                                                if (currentSlot === slot) _uiState.value = _uiState.value.copy(
                                                    questionSubmitting = false,
                                                    error = event.error ?: "回答未被接受",
                                                )
                                            }
                                        }
                                    }
                                    ChatEvent.Aborted -> {
                                        slot.pendingQuestion = null
                                        slot.questionSubmitting = false
                                        if (currentSlot === slot) _uiState.value = _uiState.value.copy(
                                            pendingQuestion = null,
                                            questionSubmitting = false,
                                        )
                                    }
                                    else -> Unit
                                }
                            }
                        }
                    }
                }

                // Observe accumulator state
                observeAccumulator(slot)
                observeConnectionState(session)
                observeAuthEvents()
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                _uiState.value = _uiState.value.copy(error = e.message)
            }
        }
    }

    private fun registerTools(session: ClawSeedSession) {
        session.tools.register(
            name = "attachment_read",
            description = "读取当前会话已发送的文件。start 从 0 开始；文本/DOCX 按 Unicode 字符，CSV 按完整记录（第 0 条是表头），PDF 按页。返回 next/eof；继续读取使用 next。原始 Android 客户端必须在线。",
            parameters = """{"type":"object","properties":{"attachment_id":{"type":"string"},"start":{"type":"integer","minimum":0},"count":{"type":"integer","minimum":1,"maximum":16000}},"required":["attachment_id","start","count"],"additionalProperties":false}""",
        ) { args ->
            try {
                val sessionId = session.sessionInfo.value?.sessionId ?: error("会话未连接")
                val key = ChatImageDrafts.key(session.gateway, sessionId)
                val id = args["attachment_id"]?.jsonPrimitive?.content ?: error("缺少 attachment_id")
                val start = args["start"]?.jsonPrimitive?.content?.toIntOrNull() ?: error("start 必须是整数")
                val count = args["count"]?.jsonPrimitive?.content?.toIntOrNull() ?: error("count 必须是整数")
                val result = fileStore.read(key, id, start, count)
                ToolResult.Success(kotlinx.serialization.json.Json.encodeToString(dev.clawseed.sdk.core.model.AttachmentReadResult.serializer(), result))
            } catch (error: Exception) {
                if (error is CancellationException) throw error
                ToolResult.Failure(error.message ?: "附件读取失败")
            }
        }
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
            description = "在设备上设置应用内闹钟（会响铃和震动唤醒用户）。适用于用户需要被闹钟叫醒或提醒的场景。" +
                "参数：hour（小时0-23）、minute（分钟0-59）、message（闹钟标签，可选）、repeat_days（重复的星期几1-7对应周一到周日，可选，空表示一次性闹钟）",
            parameters = """{"type":"object","properties":{"hour":{"type":"integer","description":"闹钟小时（0-23）","minimum":0,"maximum":23},"minute":{"type":"integer","description":"闹钟分钟（0-59）","minimum":0,"maximum":59},"message":{"type":"string","description":"闹钟标签/备注信息"},"repeat_days":{"type":"array","description":"重复的星期几（1=周一，2=周二，...7=周日），空数组或null表示一次性闹钟","items":{"type":"integer","minimum":1,"maximum":7}}},"required":["hour","minute"]}""",
        ) { args ->
            handleSetAlarm(args)
        }
    }

    fun answerQuestion(status: String, answer: JsonElement? = null) {
        val slot = currentSlot ?: return
        val request = slot.pendingQuestion ?: return
        if (slot.questionSubmitting) return
        slot.questionSubmitting = true
        _uiState.value = _uiState.value.copy(questionSubmitting = true)
        runCatching { slot.session.answerQuestion(request, status, answer) }
            .onFailure { error ->
                slot.questionSubmitting = false
                _uiState.value = _uiState.value.copy(
                    questionSubmitting = false,
                    error = error.message ?: "回答发送失败",
                )
            }
    }

    private suspend fun loadHistory(session: ClawSeedSession, sessionId: String): List<ChatEntry> {
        return withContext(Dispatchers.Default) {
            session.gateway.sessionMessages(sessionId).map { msgs ->
                fileStore.load()
                fileStore.reconcile(ChatImageDrafts.key(session.gateway, sessionId), msgs.flatMap { it.files }.map { it.id }.toSet())

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
                                    attachments = msg.attachments,
                        files = msg.files,
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
                                presentation = parseToolPresentation(msg.presentation),
                                metrics = msg.metrics,
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

                groupToolCalls(intermediate)
            }.getOrThrow()
        }
    }

    private fun observeAccumulator(slot: SessionSlot) {
        val acc = slot.accumulator
        accumulatorObservationJob?.cancel()
        // Fresh accumulator for a (possibly different) session — stop any playback and seed the
        // dedupe baseline with the last already-known assistant message so switching into a
        // session doesn't immediately speak its pre-existing history.
        tts.stop()
        ttsLastSpokenMsgId = acc.messages.value.lastOrNull()
            ?.let { (it as? dev.clawseed.sdk.android.AccumulatedMessage.Assistant)?.id }
        accumulatorObservationJob = viewModelScope.launch {
            launch {
                acc.isGenerating.collect { generating ->
                    _uiState.value = _uiState.value.copy(isGenerating = generating)
                }
            }
            launch {
                acc.error.collect { error ->
                    _uiState.value = _uiState.value.copy(error = error)
                }
            }
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
                    _uiState.value = _uiState.value.copy(
                        messages = slot.messages(),
                    )
                    // Speak the authoritative finalized assistant message once it lands.
                    val lastAcc = accumulated.lastOrNull()
                    if (lastAcc is dev.clawseed.sdk.android.AccumulatedMessage.Assistant &&
                        lastAcc.id != ttsLastSpokenMsgId
                    ) {
                        speakAssistantMessage(lastAcc)
                    }
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
                    if (state != ConnectionState.CONNECTED && accumulator?.isGenerating?.value == true) {
                        accumulator?.failTurn(getApplication<Application>().getString(R.string.chat_connection_interrupted))
                    }
                    val error = if (state == ConnectionState.CONNECTED) null else _uiState.value.error
                    _uiState.value = _uiState.value.copy(connState = state, error = error)
                }
            }
            launch {
                session.sessionInfo.collect { info ->
                    if (info != null && migrateNewDraft) {
                        draftStore.moveNewDraft(info.sessionId)
                        migrateNewDraft = false
                    }
                    _uiState.value = _uiState.value.copy(
                        sessionName = info?.name ?: _uiState.value.sessionName,
                        currentSessionId = info?.sessionId,
                        // Persona is echoed by the gateway on every connect/resume;
                        // null is a valid value (default global agent), only set
                        // when we actually have session info so the chip clears on disconnect.
                        currentPersona = info?.persona,
                        imageAttachmentsSupported = info?.imageAttachmentsSupported == true,
                        fileAttachmentsSupported = info?.fileAttachmentsSupported == true,
                    )
                    // Update pool slot sessionId if the gateway assigned a new one
                    val sid = info?.sessionId
                    if (sid != null && currentSessionId != sid) {
                        val oldId = currentSessionId
                        currentSessionId = sid
                        val slot = sessionSlots.remove(oldId)
                        if (slot != null && slot.session === session) {
                            // Migrate slot entry from old key to real sessionId
                            sessionSlots[sid] = slot
                        } else {
                            currentSlot?.let { sessionSlots[sid] = it }
                        }
                    }
                }
            }
        }
    }

    fun sendMessage(content: String, expectedSessionId: String? = null): Boolean =
        currentSlot?.sendMessage(content, debugEnabled, expectedSessionId) ?: false

    fun regenerateLastResponse() {
        val session = currentSession ?: return
        if (accumulator?.isGenerating?.value == true || session.connectionState.value != ConnectionState.CONNECTED) return
        // Stop any in-progress playback of the old reply before it's replaced.
        tts.stop()
        currentSlot?.regenerate(debugEnabled)
        _uiState.value = _uiState.value.copy(messages = currentSlot?.messages().orEmpty(), error = null)
    }

    fun retryLastRequest() {
        val sid = currentSessionId ?: _uiState.value.currentSessionId
        if (currentSession?.connectionState?.value != ConnectionState.CONNECTED || currentSlot == null) {
            switchToSession(sid)
            return
        }
        clearError()
        if (_uiState.value.messages.any { it is ChatEntry.UserMessage }) regenerateLastResponse()
    }

    suspend fun awaitSessionReady(expectedSessionId: String?) {
        connectJob?.join()
        uiState.first {
            it.connState == ConnectionState.CONNECTED && it.currentSessionId != null && currentSlot != null &&
                (expectedSessionId == null || it.currentSessionId == expectedSessionId)
        }
    }

    fun reportConnectionTimeout() {
        _uiState.value = _uiState.value.copy(
            error = _uiState.value.error ?: getApplication<Application>().getString(R.string.chat_connection_interrupted),
        )
    }

    fun abortGeneration() {
        val acc = accumulator ?: return
        if (abortJob?.isActive == true && abortingAccumulator === acc) return
        tts.stop()
        val session = currentSession
        abortingAccumulator = acc
        val generationId = acc.generationId
        abortJob = viewModelScope.launch {
            try {
                kotlinx.coroutines.withTimeoutOrNull(5_000) { session?.abort() }
            } finally {
                // A delayed REST abort must not clear a newer turn's local state.
                acc.finishTurnIfCurrent(generationId)
            }
        }
    }

    fun clearError() {
        accumulator?.clearError()
        _uiState.value = _uiState.value.copy(error = null)
    }

    fun dismissAuthPrompt() {
        _uiState.value.authPrompt?.requestId?.let {
            ClawSeedAndroid.externalToolBridge().cancelPendingAction(it)
        }
        _uiState.value = _uiState.value.copy(authPrompt = null)
        showNextAuthPrompt()
    }

    fun markProviderActionStarted(requestId: String) {
        activeProviderRequestId = requestId
        _uiState.value = _uiState.value.copy(authPrompt = null)
    }

    fun completeProviderAction() {
        activeProviderRequestId?.let {
            ClawSeedAndroid.externalToolBridge().resumePendingAction(it)
        }
        activeProviderRequestId = null
        showNextAuthPrompt()
    }

    fun failProviderAction(requestId: String? = activeProviderRequestId) {
        requestId?.let { ClawSeedAndroid.externalToolBridge().cancelPendingAction(it) }
        activeProviderRequestId = null
        _uiState.value = _uiState.value.copy(
            authPrompt = null,
            error = getApplication<Application>().getString(R.string.chat_auth_launch_failed),
        )
        showNextAuthPrompt()
    }

    fun handleAuthAction() {
        val prompt = _uiState.value.authPrompt ?: return
        val launched = runCatching {
            if (prompt.resolution != null) {
                prompt.resolution.send()
                true
            } else if (prompt.authorizeIntent != null) {
                val intent = android.content.Intent(prompt.authorizeIntent).apply {
                    setPackage(prompt.providerPackageName)
                    addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                getApplication<Application>().startActivity(intent)
                true
            } else {
                false
            }
        }.getOrDefault(false)
        _uiState.value = if (launched) {
            _uiState.value.copy(authPrompt = null)
        } else {
            _uiState.value.copy(
                authPrompt = null,
                error = getApplication<Application>().getString(R.string.chat_auth_launch_failed),
            )
        }
        showNextAuthPrompt()
    }

    private fun observeAuthEvents() {
        authEventJob?.cancel()
        authEventJob = viewModelScope.launch {
            ClawSeedAndroid.externalToolBridge().authEvents.collect { event ->
                val prompt = AuthPrompt(
                    providerPackageName = event.providerPackageName,
                    hint = event.resolutionHint ?: getApplication<Application>().getString(R.string.chat_auth_hint, event.providerLabel),
                    authorizeIntent = event.authorizeIntent,
                    resolution = event.resolution,
                    requestId = event.requestId,
                    errorCode = event.errorCode,
                )
                if (_uiState.value.authPrompt == null && activeProviderRequestId == null) {
                    _uiState.value = _uiState.value.copy(authPrompt = prompt)
                } else {
                    queuedAuthPrompts.addLast(prompt)
                }
            }
        }
    }

    private fun showNextAuthPrompt() {
        if (_uiState.value.authPrompt != null || activeProviderRequestId != null) return
        val next = queuedAuthPrompts.removeFirstOrNull() ?: return
        _uiState.value = _uiState.value.copy(authPrompt = next)
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
        val repeatDays = try {
            AlarmSchedule.parseDays(args["repeat_days"])
        } catch (e: IllegalArgumentException) {
            return ToolResult.Failure(e.message ?: "Invalid repeat_days")
        }
        val repeat = AlarmSchedule.repeat(repeatDays)

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
            repeatDays = repeatDays,
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
            put("repeat_days", kotlinx.serialization.json.JsonArray(repeatDays.map { kotlinx.serialization.json.JsonPrimitive(it) }))
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

internal class SessionSwitchVersionGate {
    private var lastVersion: Int? = null

    fun tryAcquire(version: Int): Boolean {
        if (lastVersion == version) return false
        lastVersion = version
        return true
    }
}
