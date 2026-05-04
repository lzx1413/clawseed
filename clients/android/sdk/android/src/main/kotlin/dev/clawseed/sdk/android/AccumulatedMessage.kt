package dev.clawseed.sdk.android

/** Message models emitted by [ChatAccumulator]. */
sealed class AccumulatedMessage {
    abstract val id: String
    abstract val timestamp: Long

    /** User-authored message. */
    data class User(
        override val id: String,
        override val timestamp: Long,
        val content: String,
    ) : AccumulatedMessage()

    /** Completed assistant message. */
    data class Assistant(
        override val id: String,
        override val timestamp: Long,
        val content: String,
    ) : AccumulatedMessage()

    /** Server-side tool invocation entry. */
    data class ToolCall(
        override val id: String,
        override val timestamp: Long,
        val callId: String,
        val name: String,
        val args: String,
    ) : AccumulatedMessage()

    /** Server-side tool result entry. */
    data class ToolResult(
        override val id: String,
        override val timestamp: Long,
        val callId: String,
        val name: String,
        val output: String,
    ) : AccumulatedMessage()

    /** Reasoning or thinking block captured before flush. */
    data class Thinking(
        override val id: String,
        override val timestamp: Long,
        val content: String,
    ) : AccumulatedMessage()

    /** System-level informational message, such as abort acknowledgement. */
    data class System(
        override val id: String,
        override val timestamp: Long,
        val content: String,
    ) : AccumulatedMessage()

    /** Error surfaced into accumulated UI state. */
    data class Error(
        override val id: String,
        override val timestamp: Long,
        val message: String,
    ) : AccumulatedMessage()

    /** Debug prompt payload captured when debug mode is enabled. */
    data class Debug(
        override val id: String,
        override val timestamp: Long,
        val messagesJson: String,
        val estimatedTokens: Int,
    ) : AccumulatedMessage()
}
