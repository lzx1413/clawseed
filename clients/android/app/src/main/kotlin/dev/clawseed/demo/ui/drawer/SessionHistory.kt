package dev.clawseed.demo.ui.drawer

import dev.clawseed.sdk.core.model.SessionSummary
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

internal data class SessionDay(val date: LocalDate?, val sessions: List<SessionSummary>)

internal fun groupSessionHistory(
    sessions: List<SessionSummary>,
    query: String,
    zone: ZoneId = ZoneId.systemDefault(),
): List<SessionDay> {
    val term = query.trim()
    return sessions.filter {
        term.isEmpty() || it.name.orEmpty().contains(term, ignoreCase = true) ||
            it.persona.orEmpty().contains(term, ignoreCase = true) || it.id.contains(term, ignoreCase = true)
    }.sortedByDescending { it.lastActivityMillis.takeIf { time -> time > 0 } ?: it.createdAtMillis }
        .groupBy {
            val time = it.lastActivityMillis.takeIf { time -> time > 0 } ?: it.createdAtMillis
            if (time > 0) Instant.ofEpochMilli(time).atZone(zone).toLocalDate() else null
        }.map { (date, entries) -> SessionDay(date, entries) }
}
