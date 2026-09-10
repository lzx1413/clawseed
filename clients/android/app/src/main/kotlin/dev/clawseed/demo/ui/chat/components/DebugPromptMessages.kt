package dev.clawseed.demo.ui.chat.components

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

internal enum class DebugPromptScope { System, History, Current }

internal data class DebugPromptMessage(
    val scope: DebugPromptScope,
    val role: String,
    val json: String,
)

private val debugJson = Json { prettyPrint = true }

/** Label the actual request in its original order without modifying message contents. */
internal fun debugPromptMessages(raw: String): List<DebugPromptMessage>? = runCatching {
    val messages = debugJson.parseToJsonElement(raw) as? JsonArray ?: return null
    fun role(index: Int) = ((messages[index] as? JsonObject)?.get("role") as? JsonPrimitive)?.contentOrNull.orEmpty()
    val currentUser = messages.indices.lastOrNull { role(it) == "user" }
    messages.mapIndexed { index, message ->
        val role = role(index)
        DebugPromptMessage(
            scope = when {
                role == "system" || role == "developer" -> DebugPromptScope.System
                currentUser != null && index >= currentUser -> DebugPromptScope.Current
                else -> DebugPromptScope.History
            },
            role = role,
            json = debugJson.encodeToString(kotlinx.serialization.json.JsonElement.serializer(), message),
        )
    }
}.getOrNull()
