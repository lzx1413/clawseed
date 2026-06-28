package dev.clawseed.sdk.android

import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.model.ChatEvent
import kotlinx.coroutines.CoroutineScope
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

    private val _messages = MutableStateFlow<List<AccumulatedMessage>>(emptyList())
    /** Completed message history accumulated from chat events. */
    val messages: StateFlow<List<AccumulatedMessage>> = _messages.asStateFlow()

    private val _sessionTitle = MutableStateFlow<String?>(null)
    /** Latest session title announced by the gateway. */
    val sessionTitle: StateFlow<String?> = _sessionTitle.asStateFlow()

    private val idCounter = AtomicLong(0)
    private var collectionJob: kotlinx.coroutines.Job? = null
    private var currentTurnFlushed = false
    private var regenerating = false

    /** Starts collecting [session] events inside [scope]. */
    fun startIn(scope: CoroutineScope) {
        collectionJob?.cancel()
        collectionJob = scope.launch {
            session.events.collect { event -> handleEvent(event) }
        }
    }

    /** Records a local user message so UI state stays aligned with sent input.
     *  Clears streaming buffers defensively — a new user turn always starts fresh,
     *  preventing any residual content from a previous turn leaking into the next. */
    fun addUserMessage(content: String) {
        _streamingContent.value = ""
        _thinkingContent.value = ""
        currentTurnFlushed = false
        append(AccumulatedMessage.User(
            id = nextId(),
            timestamp = System.currentTimeMillis(),
            content = content,
        ))
    }

    /** Prepares the accumulator for a regenerate: clears the last assistant turn but keeps the user message.
     *  Also clears streaming buffers so the regenerated response starts fresh. */
    fun prepareRegenerate() {
        val messages = _messages.value
        val lastUserIndex = messages.indexOfLast { it is AccumulatedMessage.User }
        if (lastUserIndex < 0) return
        // Remove everything after the last user message (assistant responses, tool calls, etc.)
        // but keep the user message itself — the server does NOT re-emit it as a ChatEvent.
        _messages.value = messages.subList(0, lastUserIndex + 1).toList()
        _streamingContent.value = ""
        _thinkingContent.value = ""
        regenerating = true
        currentTurnFlushed = false
    }

    /** Clears all accumulated state for session switching or full reset. */
    fun reset() {
        _streamingContent.value = ""
        _thinkingContent.value = ""
        _messages.value = emptyList()
        _sessionTitle.value = null
        idCounter.set(0)
        currentTurnFlushed = false
        regenerating = false
    }

    private fun handleEvent(event: ChatEvent) {
        when (event) {
            is ChatEvent.TextChunk -> {
                currentTurnFlushed = false
                _streamingContent.value += event.content
            }
            is ChatEvent.ThinkingChunk -> {
                currentTurnFlushed = false
                _thinkingContent.value += event.content
            }
            is ChatEvent.ChunkReset -> {
                // The gateway sends chunk_reset immediately before the
                // authoritative done event so clients can discard any
                // provisional draft text collected during tool use.
                _streamingContent.value = ""
                currentTurnFlushed = false
            }
            is ChatEvent.Done -> {
                val hasPendingBuffers = _streamingContent.value.isNotEmpty() || _thinkingContent.value.isNotEmpty()
                if (hasPendingBuffers || !currentTurnFlushed) {
                    flushBuffers(event.fullResponse)
                } else {
                    reconcileCompletedAssistantMessage(event.fullResponse)
                }
                currentTurnFlushed = true
            }
            is ChatEvent.ToolCallStarted -> {
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
                ))
            }
            is ChatEvent.Aborted -> {
                _streamingContent.value = ""
                _thinkingContent.value = ""
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
        val thinking = _thinkingContent.value
        if (thinking.isNotEmpty()) {
            append(AccumulatedMessage.Thinking(
                id = nextId(),
                timestamp = System.currentTimeMillis(),
                content = thinking,
            ))
            _thinkingContent.value = ""
        }
        val streaming = _streamingContent.value.ifEmpty { fullResponseFallback.orEmpty() }
        if (streaming.isNotEmpty()) {
            append(AccumulatedMessage.Assistant(
                id = nextId(),
                timestamp = System.currentTimeMillis(),
                content = streaming,
            ))
            _streamingContent.value = ""
        }
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
