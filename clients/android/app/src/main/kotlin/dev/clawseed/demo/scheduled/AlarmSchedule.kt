package dev.clawseed.demo.scheduled

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

internal object AlarmSchedule {
    fun parseDays(value: JsonElement?): List<Int> {
        if (value == null || value == JsonNull) return emptyList()
        require(value is JsonArray) { "repeat_days must be an array of weekdays (1-7)" }
        return value.map {
            val day = (it as? JsonPrimitive)?.intOrNull
            require(day != null && day in 1..7) { "repeat_days must contain weekdays from 1 to 7" }
            day
        }.distinct().sorted()
    }

    fun repeat(days: List<Int>): TaskRepeat = when (days.toSet()) {
        emptySet<Int>() -> TaskRepeat.ONCE
        (1..7).toSet() -> TaskRepeat.DAILY
        (1..5).toSet() -> TaskRepeat.WEEKDAY
        else -> TaskRepeat.CUSTOM
    }
}
