package dev.clawseed.sdk.core.tool

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject

/** Registry of remote-callable tools exposed by the client. */
class ToolRegistry {
    private val lock = Any()
    private val tools = mutableMapOf<String, ClawSeedTool>()
    private val toolRegisteredListeners = mutableListOf<() -> Unit>()

    /** Registers a tool implementation instance. */
    fun register(tool: ClawSeedTool) {
        synchronized(lock) {
            tools[tool.name] = tool
        }
        notifyToolRegistered()
    }

    /**
     * Registers a tool using a simple suspending lambda handler.
     */
    fun register(
        name: String,
        description: String,
        parameters: String,
        handler: suspend (JsonObject) -> ToolResult,
    ) {
        val params = kotlinx.serialization.json.Json.parseToJsonElement(parameters).jsonObject
        synchronized(lock) {
            tools[name] = LambdaTool(name, description, params, handler)
        }
        notifyToolRegistered()
    }

    /** Removes a tool by name. */
    fun unregister(name: String): Boolean = synchronized(lock) {
        tools.remove(name) != null
    }

    /** Returns the currently registered tool specifications. */
    fun registeredTools(): List<ToolSpec> = synchronized(lock) {
        tools.values.map {
            ToolSpec(it.name, it.description, it.parametersSchema)
        }
    }

    internal fun get(name: String): ClawSeedTool? = synchronized(lock) {
        tools[name]
    }

    internal fun clear() {
        synchronized(lock) {
            tools.clear()
        }
    }

    internal fun onToolRegistered(listener: () -> Unit) {
        synchronized(lock) {
            toolRegisteredListeners += listener
        }
    }

    private fun notifyToolRegistered() {
        val listeners = synchronized(lock) { toolRegisteredListeners.toList() }
        listeners.forEach { it() }
    }

    private class LambdaTool(
        override val name: String,
        override val description: String,
        override val parametersSchema: JsonObject,
        private val handler: suspend (JsonObject) -> ToolResult,
    ) : ClawSeedTool {
        override suspend fun execute(args: JsonObject): ToolResult = handler(args)
    }
}
