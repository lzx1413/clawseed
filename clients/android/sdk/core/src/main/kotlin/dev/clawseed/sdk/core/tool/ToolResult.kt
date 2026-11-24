package dev.clawseed.sdk.core.tool

import kotlinx.serialization.json.JsonObject

/** Result returned by a client-side tool execution. */
sealed class ToolResult {
    /** Successful tool execution with serialized output text. */
    data class Success(val output: String, val presentation: JsonObject? = null) : ToolResult()
    /** Failed tool execution with an error message safe to surface upstream. */
    data class Failure(val error: String) : ToolResult()
}
