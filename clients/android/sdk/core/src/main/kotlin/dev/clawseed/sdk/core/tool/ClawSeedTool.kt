package dev.clawseed.sdk.core.tool

import kotlinx.serialization.json.JsonObject

/**
 * Contract implemented by client-side tools that can be invoked remotely by the gateway.
 */
interface ClawSeedTool {
    /** Unique tool name exposed to the gateway. */
    val name: String
    /** Human-readable description shown to the model. */
    val description: String
    /** JSON Schema describing accepted arguments. */
    val parametersSchema: JsonObject
    /** Executes the tool with parsed JSON arguments. */
    suspend fun execute(args: JsonObject): ToolResult
}
