package dev.clawseed.demo.ui.profile

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

object UserProfileValueCodec {
    fun display(value: JsonElement): String =
        (value as? JsonPrimitive)
            ?.takeIf { it.isString }
            ?.contentOrNull
            ?: value.toString()

    fun parse(input: String): JsonElement {
        val trimmed = input.trim()
        if (trimmed.isEmpty()) return JsonPrimitive("")
        return runCatching { Json.parseToJsonElement(trimmed) }
            .getOrElse { JsonPrimitive(input) }
    }
}
