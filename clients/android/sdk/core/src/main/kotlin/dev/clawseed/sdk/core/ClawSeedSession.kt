package dev.clawseed.sdk.core

import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.serialization.json.JsonElement
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.Closeable

/**
 * Represents one chat session connected to a ClawSeed gateway.
 */
interface ClawSeedSession : Closeable {
    /** Current WebSocket transport state. */
    val connectionState: StateFlow<ConnectionState>
    /** Session metadata reported after connection or resume. */
    val sessionInfo: StateFlow<SessionInfo?>
    /** Current structured question, retained across UI recreation and reconnect. */
    val pendingQuestion: StateFlow<ChatEvent.AskUserRequested?>
        get() = kotlinx.coroutines.flow.MutableStateFlow(null)
    /** Raw event stream emitted by the gateway. */
    val events: SharedFlow<ChatEvent>
    /** Registry of remote-callable tools exposed by the client. */
    val tools: ToolRegistry
    /** REST client bound to the same gateway configuration. */
    val gateway: GatewayClient

    /**
     * Connects the session and optionally resumes [sessionId].
     *
     * [persona] selects a named persona (分身) for a **new** session; it is
     * sent as `?persona=` on the WebSocket URL. On resume (non-null
     * [sessionId] with an existing binding) the gateway ignores this parameter
     * and uses the stored binding — persona is write-once per session.
     */
    suspend fun connect(sessionId: String? = null, persona: String? = null)
    /** Gracefully disconnects the session from the gateway. */
    suspend fun disconnect()
    /** Sends a user message to the agent. */
    fun sendMessage(content: String, debug: Boolean = false)
    fun sendMessage(content: String, debug: Boolean, attachments: List<dev.clawseed.sdk.core.model.ImageAttachment>) {
        check(attachments.isEmpty()) { "This session implementation does not support images" }
        sendMessage(content, debug)
    }
    fun sendMessage(content: String, debug: Boolean, attachments: List<dev.clawseed.sdk.core.model.ImageAttachment>, files: List<dev.clawseed.sdk.core.model.FileAttachment>) {
        check(files.isEmpty()) { "This session implementation does not support files" }
        sendMessage(content, debug, attachments)
    }
    /** Requests the agent to regenerate its last response. */
    fun regenerate(debug: Boolean = false)
    /** Requests cancellation of the current agent turn. */
    suspend fun abort()
    /** Answers a pending structured question from the agent. */
    fun answerQuestion(request: ChatEvent.AskUserRequested, status: String, answer: JsonElement? = null) {
        error("This session implementation does not support structured questions")
    }

    override fun close() {
        // Default: no-op, subclasses manage their own lifecycle
    }
}
