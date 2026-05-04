package dev.clawseed.sdk.core.model

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Streaming events emitted by the ClawSeed gateway chat protocol. */
sealed class ChatEvent {
    /** Session established or resumed successfully. */
    data class SessionStarted(
        val sessionId: String,
        val name: String?,
        val resumed: Boolean,
        val messageCount: Int,
        val version: Int?,
    ) : ChatEvent()

    /** WebSocket connection acknowledged by the gateway. */
    data class Connected(val message: String, val version: Int?) : ChatEvent()

    /** Incremental assistant text delta. */
    data class TextChunk(val content: String) : ChatEvent()

    /** Incremental reasoning or thinking delta. */
    data class ThinkingChunk(val content: String) : ChatEvent()

    /** Indicates buffered text should be flushed as a completed segment. */
    data object ChunkReset : ChatEvent()

    /** Final turn completion event containing the assembled response. */
    data class Done(val fullResponse: String) : ChatEvent()

    /** Informational event describing a server-side tool invocation. */
    data class ToolCallStarted(
        val id: String,
        val name: String,
        val args: JsonObject,
    ) : ChatEvent()

    /** Informational event describing a server-side tool result. */
    data class ToolCallCompleted(
        val id: String,
        val name: String,
        val output: String,
    ) : ChatEvent()

    /** Requests the client to execute a registered remote tool. */
    data class ToolCallRequested(
        val id: String,
        val name: String,
        val args: JsonObject,
    ) : ChatEvent()

    /** Confirms remote tool registration on the gateway. */
    data class ToolsRegistered(val count: Int, val registered: Int) : ChatEvent()

    /** Confirms that a client-sent tool result was received. */
    data class ResultAcknowledged(val id: String) : ChatEvent()

    /** Indicates the current generation was aborted. */
    data object Aborted : ChatEvent()

    /** Session title changed on the gateway. */
    data class TitleUpdated(val title: String) : ChatEvent()

    /** Gateway-side error surfaced through the chat stream. */
    data class Error(val message: String, val code: String? = null) : ChatEvent()

    /** Extra debug payload emitted when debug mode is enabled. */
    data class DebugPrompt(val messages: String, val estimatedTokens: Int) : ChatEvent()

    companion object {
        internal fun parse(text: String, json: kotlinx.serialization.json.Json): ChatEvent? {
            val element = runCatching { json.parseToJsonElement(text) }.getOrElse {
                return ChatEvent.Error("Unparseable frame")
            }
            val obj = element.jsonObject
            val type = obj["type"]?.jsonPrimitive?.content ?: return null
            return when (type) {
                "session_start" -> SessionStarted(
                    sessionId = obj["session_id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content?.takeIf { it.isNotEmpty() },
                    resumed = obj["resumed"]?.jsonPrimitive?.booleanOrNull ?: false,
                    messageCount = obj["message_count"]?.jsonPrimitive?.intOrNull ?: 0,
                    version = obj["v"]?.jsonPrimitive?.intOrNull,
                )
                "connected" -> Connected(
                    message = obj["message"]?.jsonPrimitive?.content ?: "",
                    version = obj["v"]?.jsonPrimitive?.intOrNull,
                )
                "chunk" -> TextChunk(obj["content"]?.jsonPrimitive?.content ?: "")
                "thinking" -> ThinkingChunk(obj["content"]?.jsonPrimitive?.content ?: "")
                "done" -> Done(obj["full_response"]?.jsonPrimitive?.content ?: "")
                "tool_call" -> ToolCallStarted(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content ?: "",
                    args = obj["args"]?.jsonObject ?: buildJsonObject {},
                )
                "tool_result" -> ToolCallCompleted(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content ?: "",
                    output = obj["output"]?.jsonPrimitive?.content ?: "",
                )
                "tool_call_request" -> ToolCallRequested(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content ?: "",
                    args = obj["args"]?.jsonObject ?: buildJsonObject {},
                )
                "tools_registered" -> ToolsRegistered(
                    count = obj["count"]?.jsonPrimitive?.intOrNull ?: 0,
                    registered = obj["registered"]?.jsonPrimitive?.intOrNull ?: 0,
                )
                "result_acknowledged" -> ResultAcknowledged(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                )
                "chunk_reset" -> ChunkReset
                "aborted" -> Aborted
                "title_updated" -> TitleUpdated(obj["title"]?.jsonPrimitive?.content ?: "")
                "error" -> Error(
                    message = obj["message"]?.jsonPrimitive?.content ?: "Unknown error",
                    code = obj["code"]?.jsonPrimitive?.content?.takeIf { it.isNotEmpty() },
                )
                "debug_prompt" -> DebugPrompt(
                    messages = obj["messages"]?.jsonPrimitive?.content ?: "",
                    estimatedTokens = obj["estimated_tokens"]?.jsonPrimitive?.intOrNull ?: 0,
                )
                else -> null
            }
        }
    }
}
