package dev.clawseed.sdk.core.tool

import kotlinx.serialization.json.JsonObject

/** Public description of a tool registered with [ToolRegistry]. */
data class ToolSpec(
    /** Unique tool name. */
    val name: String,
    /** Human-readable description presented to the model. */
    val description: String,
    /** JSON Schema for the tool arguments. */
    val parameters: JsonObject,
)
