package dev.clawseed.sdk.core

import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
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
    /** Raw event stream emitted by the gateway. */
    val events: SharedFlow<ChatEvent>
    /** Registry of remote-callable tools exposed by the client. */
    val tools: ToolRegistry
    /** REST client bound to the same gateway configuration. */
    val gateway: GatewayClient

    /** Connects the session and optionally resumes [sessionId]. */
    suspend fun connect(sessionId: String? = null)
    /** Gracefully disconnects the session from the gateway. */
    suspend fun disconnect()
    /** Sends a user message to the agent. */
    fun sendMessage(content: String, debug: Boolean = false)
    /** Requests cancellation of the current agent turn. */
    suspend fun abort()

    override fun close() {
        // Default: no-op, subclasses manage their own lifecycle
    }
}
