package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Metadata about the currently connected session. */
@Serializable
data class SessionInfo(
    val sessionId: String,
    val name: String?,
    val resumed: Boolean,
    val messageCount: Int,
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

/** One persisted message record returned by the session history API. */
@Serializable
data class SessionMessage(
    val role: String,
    val content: String? = null,
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
