package dev.clawseed.demo.ui.drawer

import dev.clawseed.sdk.core.model.SessionSummary
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId

/** Sessions grouped by the persona that owns their conversation history. */
internal data class SessionPersonaGroup(
    val persona: String?,
    val sessions: List<SessionSummary>,
    val pinnedCount: Int = 0,
) {
    // Prefix named personas so a real name cannot collide with the default group.
    val key: String get() = persona?.let { "persona:$it" } ?: "default"
}

internal data class SessionHistory(
    val pinned: List<SessionSummary>,
    val groups: List<SessionPersonaGroup>,
)

internal fun sessionPersona(session: SessionSummary): String? = session.persona?.takeIf(String::isNotBlank)

/** Snapshot all sessions of a persona, independently of search, pinning and collapse. */
internal fun personaSessionIds(sessions: List<SessionSummary>, persona: String?): List<String> =
    sessions.filter { sessionPersona(it) == persona?.takeIf(String::isNotBlank) }.map { it.id }.distinct()

private fun matchesSession(session: SessionSummary, term: String): Boolean {
    return session.messageCount > 0 && (term.isEmpty() || session.name.orEmpty().contains(term, ignoreCase = true) ||
        session.persona.orEmpty().contains(term, ignoreCase = true) || session.id.contains(term, ignoreCase = true))
}

private fun sessionActivityMillis(session: SessionSummary): Long =
    session.lastActivityMillis.takeIf { it > 0 } ?: session.createdAtMillis

/** Filters and sorts history into collapsible persona sections. */
internal fun buildSessionHistory(
    sessions: List<SessionSummary>,
    query: String,
    pinnedSessionIds: Set<String> = emptySet(),
): SessionHistory {
    val term = query.trim()
    val sorted = sessions.asSequence()
        .filter { matchesSession(it, term) }
        .map { it to sessionActivityMillis(it) }
        .sortedWith(
            compareByDescending<Pair<SessionSummary, Long>> { it.second }
                .thenBy { it.first.id },
        )
        .map { it.first }
        .toList()
    return SessionHistory(
        pinned = sorted.filter { it.id in pinnedSessionIds },
        groups = sorted.groupBy(::sessionPersona).map { (persona, entries) ->
            // Keep headers for personas with only pins so bulk actions remain reachable.
            SessionPersonaGroup(
                persona,
                entries.filter { it.id !in pinnedSessionIds },
                entries.count { it.id in pinnedSessionIds },
            )
        },
    )
}

internal fun sessionHistoryDate(
    session: SessionSummary,
    zone: ZoneId = ZoneId.systemDefault(),
): LocalDate? {
    val time = sessionActivityMillis(session)
    return if (time > 0) Instant.ofEpochMilli(time).atZone(zone).toLocalDate() else null
}
