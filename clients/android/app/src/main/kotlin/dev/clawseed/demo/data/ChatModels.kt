package dev.clawseed.demo.data

enum class TurnState { IDLE, STREAMING_TEXT, EXPECTING_RESULT, ERROR }

sealed class ChatEntry {
    abstract val id: String
    abstract val timestamp: Long

    data class UserMessage(
        override val id: String,
        override val timestamp: Long,
        val content: String,
    ) : ChatEntry()

    data class AssistantMessage(
        override val id: String,
        override val timestamp: Long,
        val content: String,
        val isStreaming: Boolean = false,
    ) : ChatEntry()

    data class ToolCall(
        override val id: String,
        override val timestamp: Long,
        val toolCallId: String,
        val toolName: String,
        val toolArgs: String,
    ) : ChatEntry()

    data class ToolResult(
        override val id: String,
        override val timestamp: Long,
        val toolCallId: String,
        val toolName: String,
        val toolResult: String,
        val toolSuccess: Boolean,
    ) : ChatEntry()

    data class Thinking(
        override val id: String,
        override val timestamp: Long,
        val content: String,
    ) : ChatEntry()

    data class DebugInfo(
        override val id: String,
        override val timestamp: Long,
        val messagesJson: String,
        val estimatedTokens: Int,
    ) : ChatEntry()
}

data class ChatSession(
    val id: String,
    val name: String?,
    val createdAt: Long,
    val lastActivity: Long,
    val messageCount: Int,
)

data class ToolInfo(
    val name: String = "",
    val description: String = "",
    val source_type: String = "builtin",
    val source: String? = null,
)

data class StatusInfo(
    val provider: String? = null,
    val model: String = "",
    val temperature: Double = 0.7,
    val memory_backend: String? = null,
    val paired: Boolean = false,
    val gateway_port: Int = 0,
)
