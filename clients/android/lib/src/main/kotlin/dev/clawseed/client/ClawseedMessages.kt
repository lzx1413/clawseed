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
    data class Chunk(val text: String) : IncomingMessage()
    data class Done(val finalText: String) : IncomingMessage()
    data class ToolCallRequestMsg(val request: ToolCallRequest) : IncomingMessage()
    data class Error(val message: String) : IncomingMessage()

    companion object {
        fun parse(text: String): IncomingMessage? {
            val obj = runCatching { JSONObject(text) }.getOrElse { return Error("Unparseable frame") }
            return when (obj.optString("type")) {
                "chunk" -> Chunk(obj.optString("content"))
                "done" -> Done(obj.optString("full_response"))
                "tool_call_request" -> runCatching { ToolCallRequestMsg(ToolCallRequest.fromJson(obj)) }
                    .getOrElse { Error("Malformed tool_call_request: ${it.message}") }
                "error" -> Error(obj.optString("message"))
                else -> null
            }
        }
    }
}
