package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/** Metadata about the currently connected session. */
@Serializable
data class SessionInfo(
    val sessionId: String,
    val name: String?,
    val resumed: Boolean,
    val messageCount: Int,
    /** Persona bound to this session, echoed by the gateway in session_start.
     *  Null means the default (global) agent. Write-once: on resume this
     *  reflects the stored binding, not anything the client requested. */
    val persona: String? = null,
)

/** Summary information returned by the gateway session list API. */
@Serializable
data class SessionSummary(
    @SerialName("session_id") val id: String,
    val name: String? = null,
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("last_activity") val lastActivity: String = "",
    @SerialName("message_count") val messageCount: Int = 0,
) {
    /** Creation timestamp converted to epoch milliseconds when parsing succeeds. */
    val createdAtMillis: Long get() = parseIsoToEpochMillis(createdAt)
    /** Last activity timestamp converted to epoch milliseconds when parsing succeeds. */
    val lastActivityMillis: Long get() = parseIsoToEpochMillis(lastActivity)
}

/** One persisted message record returned by the session history API.
 *
 * Supports both legacy format (role/content only) and structured format
 * (type/data with tool calls, results, and reasoning content).
 */
@Serializable
data class SessionMessage(
    val role: String,
    val content: String? = null,
    /** Message type: "chat", "assistant_tool_calls", or "tool_results". */
    val type: String = "chat",
    /** Structured payload for non-chat types. For "assistant_tool_calls" this
     *  is a JsonObject with text/tool_calls/reasoning_content; for "tool_results"
     *  it is a JsonArray of ToolResultMessage objects. Null for legacy flat messages. */
    val data: JsonElement? = null,
    // Legacy fields (no longer populated by new API, kept for backward compat)
    @SerialName("tool_name") val toolName: String? = null,
    @SerialName("tool_args") val toolArgs: String? = null,
    @SerialName("tool_result") val toolResult: String? = null,
    val success: Boolean? = null,
)

internal fun parseIsoToEpochMillis(iso: String): Long {
    return try {
        java.time.Instant.parse(iso).toEpochMilli()
    } catch (_: Exception) {
        0L
    }
}
