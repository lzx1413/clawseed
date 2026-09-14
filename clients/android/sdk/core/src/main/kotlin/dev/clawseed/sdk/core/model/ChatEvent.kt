package dev.clawseed.sdk.core.model

import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Streaming events emitted by the ClawSeed gateway chat protocol. */
sealed class ChatEvent {
    data class AskUserOption(
        val id: String,
        val label: String,
        val description: String?,
    )

    data class AskUserRequested(
        val requestId: String,
        val sessionId: String,
        val turnId: String,
        val toolCallId: String,
        val kind: String,
        val question: String,
        val options: List<AskUserOption>,
        val placeholder: String?,
    ) : ChatEvent()

    data class AskUserAcknowledged(
        val requestId: String,
        val status: String,
        val error: String?,
    ) : ChatEvent()
    /** Session established or resumed successfully. */
    data class SessionStarted(
        val sessionId: String,
        val name: String?,
        val resumed: Boolean,
        val messageCount: Int,
        val version: Int?,
        /** Persona bound to this session, echoed by the gateway. Null = default. */
        val persona: String? = null,
        val imageAttachmentsSupported: Boolean = false,
    val fileAttachmentsSupported: Boolean = false,
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
    data class Done(val fullResponse: String, val metrics: ResponseMetrics? = null) : ChatEvent()

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
        val presentation: ToolPresentation? = null,
    ) : ChatEvent()

    /** Requests the client to execute a registered remote tool. */
    data class ToolCallRequested(
        val id: String,
        val name: String,
        val args: JsonObject,
    ) : ChatEvent()

    /** Terminal state of a server-side background command. */
    data class BackgroundJobCompleted(
        val jobId: String,
        val status: String,
        val exitCode: Int?,
        val error: String?,
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

    data class ImageContext(val omittedIds: List<String>) : ChatEvent()

    /** Extra debug payload emitted when debug mode is enabled. */
    data class DebugPrompt(
        val messages: String, val estimatedTokens: Int,
        val toolsJson: String? = null, val estimatedToolTokens: Int = 0,
    ) : ChatEvent()

    companion object {
        internal fun parse(text: String, json: kotlinx.serialization.json.Json): ChatEvent? {
            val element = runCatching { json.parseToJsonElement(text) }.getOrElse {
                return ChatEvent.Error("Unparseable frame")
            }
            val obj = element.jsonObject
            val type = obj["type"]?.jsonPrimitive?.content ?: return null
            return when (type) {
                "session_start" -> SessionStarted(
                    imageAttachmentsSupported = obj["image_attachments_supported"]?.jsonPrimitive?.booleanOrNull ?: false,
                    fileAttachmentsSupported = obj["file_attachments_supported"]?.jsonPrimitive?.booleanOrNull ?: false,
                    sessionId = obj["session_id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content?.takeIf { it.isNotEmpty() },
                    resumed = obj["resumed"]?.jsonPrimitive?.booleanOrNull ?: false,
                    messageCount = obj["message_count"]?.jsonPrimitive?.intOrNull ?: 0,
                    version = obj["v"]?.jsonPrimitive?.intOrNull,
                    persona = obj["persona"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotEmpty() },
                )
                "connected" -> Connected(
                    message = obj["message"]?.jsonPrimitive?.content ?: "",
                    version = obj["v"]?.jsonPrimitive?.intOrNull,
                )
                "image_context" -> ImageContext(obj["omitted_ids"]?.jsonArray?.map { it.jsonPrimitive.content } ?: emptyList())
                "chunk" -> TextChunk(obj["content"]?.jsonPrimitive?.content ?: "")
                "thinking" -> ThinkingChunk(obj["content"]?.jsonPrimitive?.content ?: "")
                "done" -> Done(
                    fullResponse = obj["full_response"]?.jsonPrimitive?.content ?: "",
                    metrics = obj["metrics"]?.takeUnless { it is JsonNull }?.let {
                        runCatching { json.decodeFromJsonElement<ResponseMetrics>(it) }.getOrNull()
                    },
                )
                "tool_call" -> ToolCallStarted(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content ?: "",
                    args = obj["args"]?.jsonObject ?: buildJsonObject {},
                )
                "tool_result" -> ToolCallCompleted(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content ?: "",
                    output = obj["output"]?.jsonPrimitive?.content ?: "",
                    presentation = parseToolPresentation(obj["presentation"]),
                )
                "tool_call_request" -> ToolCallRequested(
                    id = obj["id"]?.jsonPrimitive?.content ?: "",
                    name = obj["name"]?.jsonPrimitive?.content ?: "",
                    args = obj["args"]?.jsonObject ?: buildJsonObject {},
                )
                "ask_user_request" -> AskUserRequested(
                    requestId = obj["request_id"]?.jsonPrimitive?.content ?: "",
                    sessionId = obj["session_id"]?.jsonPrimitive?.content ?: "",
                    turnId = obj["turn_id"]?.jsonPrimitive?.content ?: "",
                    toolCallId = obj["tool_call_id"]?.jsonPrimitive?.content ?: "",
                    kind = obj["kind"]?.jsonPrimitive?.content ?: "",
                    question = obj["question"]?.jsonPrimitive?.content ?: "",
                    options = obj["options"]?.jsonArray?.mapNotNull { option ->
                        val value = option as? JsonObject ?: return@mapNotNull null
                        val id = value["id"]?.jsonPrimitive?.content ?: return@mapNotNull null
                        val label = value["label"]?.jsonPrimitive?.content ?: return@mapNotNull null
                        AskUserOption(id, label, value["description"]?.jsonPrimitive?.contentOrNull)
                    }.orEmpty(),
                    placeholder = obj["placeholder"]?.jsonPrimitive?.contentOrNull,
                )
                "ask_user_ack" -> AskUserAcknowledged(
                    requestId = obj["request_id"]?.jsonPrimitive?.content ?: "",
                    status = obj["status"]?.jsonPrimitive?.content ?: "rejected",
                    error = obj["error"]?.jsonPrimitive?.contentOrNull,
                )
                "background_job" -> BackgroundJobCompleted(
                    jobId = obj["job_id"]?.jsonPrimitive?.content ?: "",
                    status = obj["status"]?.jsonPrimitive?.content ?: "failed",
                    exitCode = obj["exit_code"]?.jsonPrimitive?.intOrNull,
                    error = obj["error"]?.jsonPrimitive?.contentOrNull,
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
                "abort_ack" -> Aborted
                "title_updated" -> TitleUpdated(obj["title"]?.jsonPrimitive?.content ?: "")
                "error" -> Error(
                    message = obj["message"]?.jsonPrimitive?.content ?: "Unknown error",
                    code = obj["code"]?.jsonPrimitive?.content?.takeIf { it.isNotEmpty() },
                )
                "debug_prompt" -> DebugPrompt(
                    messages = obj["messages"]?.jsonPrimitive?.content ?: "",
                    estimatedTokens = obj["estimated_tokens"]?.jsonPrimitive?.intOrNull ?: 0,
                    toolsJson = obj["tools"]?.jsonPrimitive?.contentOrNull,
                    estimatedToolTokens = obj["estimated_tool_tokens"]?.jsonPrimitive?.intOrNull ?: 0,
                )
                else -> null
            }
        }
    }
}
