package dev.clawseed.sdk.android

import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.model.ChatEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicLong

/**
 * Accumulates raw [ChatEvent] values into UI-friendly streaming and history state.
 */
class ChatAccumulator(private val session: ClawSeedSession) {

    private val _streamingContent = MutableStateFlow("")
    /** Streaming assistant text for the current turn. */
    val streamingContent: StateFlow<String> = _streamingContent.asStateFlow()

    private val _thinkingContent = MutableStateFlow("")
    /** Streaming reasoning text for the current turn. */
    val thinkingContent: StateFlow<String> = _thinkingContent.asStateFlow()

    private val _isGenerating = MutableStateFlow(false)
    val isGenerating: StateFlow<Boolean> = _isGenerating.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private val _messages = MutableStateFlow<List<AccumulatedMessage>>(emptyList())
    /** Completed message history accumulated from chat events. */
    val messages: StateFlow<List<AccumulatedMessage>> = _messages.asStateFlow()

    private val _sessionTitle = MutableStateFlow<String?>(null)
    /** Latest session title announced by the gateway. */
    val sessionTitle: StateFlow<String?> = _sessionTitle.asStateFlow()

    private val idCounter = AtomicLong(0)
    private var collectionJob: Job? = null
    private var publicationJob: Job? = null
    private var collectionScope: CoroutineScope? = null
    private val textBuffer = StringBuilder()
    private val thinkingBuffer = StringBuilder()
    private var publishedFirstChunk = false

    private fun publishBuffers() {
        _streamingContent.value = textBuffer.toString()
        _thinkingContent.value = thinkingBuffer.toString()
    }

    private fun schedulePublication() {
        // Show the first chunk immediately, then merge bursts into bounded UI updates.
        if (!publishedFirstChunk) {
            publishBuffers()
            publishedFirstChunk = true
        }
        if (publicationJob == null) publicationJob = collectionScope?.launch {
            delay(40)
            publishBuffers()
            publicationJob = null
        }
    }

    private fun clearBuffers() {
        publicationJob?.cancel()
        publicationJob = null
        textBuffer.setLength(0)
        thinkingBuffer.setLength(0)
        _streamingContent.value = ""
        _thinkingContent.value = ""
        publishedFirstChunk = false
    }

    /** Release event collection when the owning session leaves the pool. */
    fun stop() {
        collectionJob?.cancel()
        collectionJob = null
        clearBuffers()
        collectionScope = null
    }
    private var currentTurnFlushed = false
    private var regenerating = false
    var generationId: Long = 0
        private set

    /** Starts collecting [session] events inside [scope]. */
    fun startIn(scope: CoroutineScope) {
        collectionJob?.cancel()
        collectionScope = scope
        collectionJob = scope.launch {
            session.events.collect { event -> handleEvent(event) }
        }
    }

    /** Records a local user message so UI state stays aligned with sent input.
     *  Clears streaming buffers defensively — a new user turn always starts fresh,
     *  preventing any residual content from a previous turn leaking into the next. */
    fun addUserMessage(content: String, attachments: List<dev.clawseed.sdk.core.model.ImageAttachment> = emptyList()) {
        beginTurn()
        append(AccumulatedMessage.User(
            id = nextId(),
            timestamp = System.currentTimeMillis(),
            content = content,
            attachments = attachments,
        ))
    }

    /** Prepares the accumulator for a regenerate: clears the last assistant turn but keeps the user message.
     *  Also clears streaming buffers so the regenerated response starts fresh. */
    fun prepareRegenerate() {
        beginTurn()
        val messages = _messages.value
        val lastUserIndex = messages.indexOfLast { it is AccumulatedMessage.User }
        if (lastUserIndex < 0) return
        // Remove everything after the last user message (assistant responses, tool calls, etc.)
        // but keep the user message itself — the server does NOT re-emit it as a ChatEvent.
        _messages.value = messages.subList(0, lastUserIndex + 1).toList()
        clearBuffers()
        regenerating = true
        currentTurnFlushed = false
    }

    /** Clears all accumulated state for session switching or full reset. */
    fun reset() {
        finishTurn()
        clearError()
        clearBuffers()
        _messages.value = emptyList()
        _sessionTitle.value = null
        idCounter.set(0)
        currentTurnFlushed = false
        regenerating = false
    }

    private fun beginTurn() {
        generationId++
        clearBuffers()
        currentTurnFlushed = false
        clearError()
        _isGenerating.value = true
    }

    fun finishTurn() {
        clearBuffers()
        _isGenerating.value = false
    }

    fun finishTurnIfCurrent(id: Long) {
        if (id == generationId) finishTurn()
    }

    fun clearError() {
        _error.value = null
    }

    fun failTurn(message: String) {
        finishTurn()
        _error.value = message
    }

    private fun handleEvent(event: ChatEvent) {
        when (event) {
            is ChatEvent.ImageContext -> {
                if (event.omittedIds.isNotEmpty()) append(AccumulatedMessage.System(
                    id = nextId(), timestamp = System.currentTimeMillis(),
                    content = "部分历史图片已超出本轮图片上下文，仍可查看；如需继续询问这些图片，请重新附图。",
                ))
            }
            is ChatEvent.TextChunk -> {
                _isGenerating.value = true
                currentTurnFlushed = false
                textBuffer.append(event.content)
                schedulePublication()
            }
            is ChatEvent.ThinkingChunk -> {
                _isGenerating.value = true
                currentTurnFlushed = false
                thinkingBuffer.append(event.content)
                schedulePublication()
            }
            is ChatEvent.ChunkReset -> {
                // The gateway sends chunk_reset immediately before the
                // authoritative done event so clients can discard any
                // provisional draft text collected during tool use.
                textBuffer.setLength(0)
                _streamingContent.value = ""
                currentTurnFlushed = false
            }
            is ChatEvent.Done -> {
                val hasPendingBuffers = textBuffer.isNotEmpty() || thinkingBuffer.isNotEmpty()
                if (hasPendingBuffers || !currentTurnFlushed) {
                    flushBuffers(event.fullResponse)
                } else {
                    reconcileCompletedAssistantMessage(event.fullResponse)
                }
                event.metrics?.let { metrics ->
                    val index = _messages.value.indexOfLast { it is AccumulatedMessage.Assistant }
                    val userIndex = _messages.value.indexOfLast { it is AccumulatedMessage.User }
                    if (index > userIndex) {
                        _messages.value = _messages.value.toMutableList().apply {
                            val assistant = get(index) as AccumulatedMessage.Assistant
                            set(index, assistant.copy(metrics = metrics))
                        }
                    }
                }
                currentTurnFlushed = true
                _isGenerating.value = false
            }
            is ChatEvent.ToolCallStarted -> {
                _isGenerating.value = true
                append(AccumulatedMessage.ToolCall(
                    id = nextId(),
                    timestamp = System.currentTimeMillis(),
                    callId = event.id,
                    name = event.name,
                    args = event.args.toString(),
                ))
            }
            is ChatEvent.ToolCallCompleted -> {
                append(AccumulatedMessage.ToolResult(
                    id = nextId(),
                    timestamp = System.currentTimeMillis(),
                    callId = event.id,
                    name = event.name,
                    output = event.output,
                    presentation = event.presentation,
                ))
            }
            is ChatEvent.Aborted -> {
                finishTurn()
                append(AccumulatedMessage.System(
                    id = nextId(),
                    timestamp = System.currentTimeMillis(),
                    content = "Generation aborted.",
                ))
                currentTurnFlushed = false
            }
            is ChatEvent.TitleUpdated -> {
                _sessionTitle.value = event.title
            }
            is ChatEvent.Error -> {
                failTurn(event.message)
                append(AccumulatedMessage.Error(
                    id = nextId(),
                    timestamp = System.currentTimeMillis(),
                    message = event.message,
                ))
            }
            is ChatEvent.DebugPrompt -> {
                append(AccumulatedMessage.Debug(
                    id = nextId(),
                    timestamp = System.currentTimeMillis(),
                    messagesJson = event.messages,
                    estimatedTokens = event.estimatedTokens,
                    toolsJson = event.toolsJson,
                    estimatedToolTokens = event.estimatedToolTokens,
                ))
            }
            // SessionStarted, Connected, ToolsRegistered, ResultAcknowledged — no accumulation needed
            is ChatEvent.SessionStarted,
            is ChatEvent.Connected,
            is ChatEvent.ToolsRegistered,
            is ChatEvent.ResultAcknowledged,
            is ChatEvent.ToolCallRequested -> {}
        }
    }

    private fun flushBuffers(fullResponseFallback: String? = null) {
        val thinking = thinkingBuffer.toString()
        if (thinking.isNotEmpty()) {
            append(AccumulatedMessage.Thinking(
                id = nextId(),
                timestamp = System.currentTimeMillis(),
                content = thinking,
            ))
            _thinkingContent.value = ""
        }
        val completedContent = fullResponseFallback
            ?.takeIf { it.isNotEmpty() }
            ?: textBuffer.toString()
        if (completedContent.isNotEmpty()) {
            append(AccumulatedMessage.Assistant(
                id = nextId(),
                timestamp = System.currentTimeMillis(),
                content = completedContent,
            ))
        }
        clearBuffers()
    }

    private fun reconcileCompletedAssistantMessage(fullResponse: String) {
        if (fullResponse.isEmpty()) {
            return
        }

        val turnStartIndex = _messages.value.indexOfLast { it is AccumulatedMessage.User }.let { index ->
            if (index >= 0) index + 1 else 0
        }
        val assistantIndices = _messages.value.indices.filter { index ->
            index >= turnStartIndex && _messages.value[index] is AccumulatedMessage.Assistant
        }

        if (assistantIndices.isEmpty()) {
            append(AccumulatedMessage.Assistant(
                id = nextId(),
                timestamp = System.currentTimeMillis(),
                content = fullResponse,
            ))
            return
        }

        val assistantContent = assistantIndices.joinToString(separator = "") { index ->
            (_messages.value[index] as AccumulatedMessage.Assistant).content
        }
        if (assistantContent == fullResponse || !fullResponse.startsWith(assistantContent)) {
            return
        }

        val lastAssistantIndex = assistantIndices.last()
        val lastAssistant = _messages.value[lastAssistantIndex] as AccumulatedMessage.Assistant
        val missingSuffix = fullResponse.removePrefix(assistantContent)
        if (missingSuffix.isEmpty()) {
            return
        }

        val updatedMessages = _messages.value.toMutableList()
        updatedMessages[lastAssistantIndex] = lastAssistant.copy(
            content = lastAssistant.content + missingSuffix,
        )
        _messages.value = updatedMessages
    }

    private fun append(msg: AccumulatedMessage) {
        _messages.value = _messages.value + msg
    }

    private fun nextId(): String = "msg-${idCounter.incrementAndGet()}"
}
