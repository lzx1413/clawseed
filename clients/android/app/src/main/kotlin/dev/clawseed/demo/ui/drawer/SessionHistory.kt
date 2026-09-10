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
    return sessions.asSequence().filter {
        // Connecting can persist persona/name metadata before the first message.
        it.messageCount > 0 && (term.isEmpty() || it.name.orEmpty().contains(term, ignoreCase = true) ||
            it.persona.orEmpty().contains(term, ignoreCase = true) || it.id.contains(term, ignoreCase = true))
    }.map { session ->
        // Parse once per session, not on every comparison in the sort.
        session to (session.lastActivityMillis.takeIf { it > 0 } ?: session.createdAtMillis)
    }.sortedByDescending { it.second }
        .groupBy(keySelector = { (_, time) ->
            if (time > 0) Instant.ofEpochMilli(time).atZone(zone).toLocalDate() else null
        }, valueTransform = { it.first })
        .map { (date, entries) -> SessionDay(date, entries) }
}
