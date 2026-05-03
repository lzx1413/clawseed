package dev.clawseed.client

import org.json.JSONObject

data class ToolSpec(
    val name: String,
    val description: String,
    val parameters: String,
) {
    fun toJson(): JSONObject = JSONObject()
        .put("name", name)
        .put("description", description)
        .put("parameters", JSONObject(parameters))
}

data class ToolCallRequest(
    val id: String,
    val name: String,
    val args: JSONObject,
) {
    companion object {
        fun fromJson(obj: JSONObject) = ToolCallRequest(
            id = obj.getString("id"),
            name = obj.getString("name"),
            args = obj.optJSONObject("args") ?: JSONObject(),
        )
    }
}

sealed class ToolCallResult {
    data class Success(val output: String) : ToolCallResult()
    data class Failure(val error: String) : ToolCallResult()

    fun toJson(callId: String): JSONObject = when (this) {
        is Success -> JSONObject()
            .put("type", "tool_result")
            .put("id", callId)
            .put("output", output)
            .put("success", true)
        is Failure -> JSONObject()
            .put("type", "tool_error")
            .put("id", callId)
            .put("error", error)
            .put("success", false)
    }
}

sealed class IncomingMessage {
    data class SessionStart(
        val sessionId: String,
        val name: String?,
        val resumed: Boolean,
        val messageCount: Int,
        val version: Int?,
    ) : IncomingMessage()

    data class Connected(val message: String, val version: Int?) : IncomingMessage()

    data class Chunk(val text: String) : IncomingMessage()
    data class Thinking(val text: String) : IncomingMessage()
    data class Done(val finalText: String) : IncomingMessage()

    data class ToolCallMsg(
        val id: String,
        val name: String,
        val args: JSONObject,
    ) : IncomingMessage()

    data class ToolResultMsg(
        val id: String,
        val name: String,
        val output: String,
    ) : IncomingMessage()

    data class ToolCallRequestMsg(val request: ToolCallRequest) : IncomingMessage()

    data class ToolsRegistered(val count: Int, val registered: Int) : IncomingMessage()
    data class ResultAcknowledged(val id: String) : IncomingMessage()

    data object ChunkReset : IncomingMessage()
    data object Aborted : IncomingMessage()
    data class Error(val message: String) : IncomingMessage()
    data class DebugPrompt(val messages: String, val estimatedTokens: Int) : IncomingMessage()

    companion object {
        fun parse(text: String): IncomingMessage? {
            val obj = runCatching { JSONObject(text) }.getOrElse { return Error("Unparseable frame") }
            return when (obj.optString("type")) {
                "session_start" -> SessionStart(
                    sessionId = obj.optString("session_id"),
                    name = obj.optString("name").takeIf { it.isNotEmpty() },
                    resumed = obj.optBoolean("resumed"),
                    messageCount = obj.optInt("message_count"),
                    version = obj.optInt("v").takeIf { obj.has("v") },
                )
                "connected" -> Connected(
                    message = obj.optString("message"),
                    version = obj.optInt("v").takeIf { obj.has("v") },
                )
                "chunk" -> Chunk(obj.optString("content"))
                "thinking" -> Thinking(obj.optString("content"))
                "done" -> Done(obj.optString("full_response"))
                "tool_call" -> ToolCallMsg(
                    id = obj.optString("id"),
                    name = obj.optString("name"),
                    args = obj.optJSONObject("args") ?: JSONObject(),
                )
                "tool_result" -> ToolResultMsg(
                    id = obj.optString("id"),
                    name = obj.optString("name"),
                    output = obj.optString("output"),
                )
                "tool_call_request" -> runCatching { ToolCallRequestMsg(ToolCallRequest.fromJson(obj)) }
                    .getOrElse { Error("Malformed tool_call_request: ${it.message}") }
                "tools_registered" -> ToolsRegistered(
                    count = obj.optInt("count"),
                    registered = obj.optInt("registered"),
                )
                "result_acknowledged" -> ResultAcknowledged(obj.optString("id"))
                "chunk_reset" -> ChunkReset
                "aborted" -> Aborted
                "debug_prompt" -> DebugPrompt(
                    messages = obj.optString("messages"),
                    estimatedTokens = obj.optInt("estimated_tokens"),
                )
                "error" -> Error(obj.optString("message"))
                else -> null
            }
        }
    }
}
