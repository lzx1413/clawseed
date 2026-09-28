package dev.clawseed.demo.ui.drawer

import dev.clawseed.sdk.core.model.SessionSummary
import java.time.LocalDate
import java.time.ZoneId
import org.junit.Assert.*
import org.junit.Test

class SessionHistoryTest {
    private fun groupSessionsByPersona(sessions: List<SessionSummary>, query: String) =
        buildSessionHistory(sessions, query).groups

    private val zone = ZoneId.of("Asia/Shanghai")

    @Test fun sortsByActivityAndDisplaysDatesInLocalTimezone() {
        val old = SessionSummary("old", lastActivity = "2026-09-06T15:00:00Z", messageCount = 2)
        val fresh = SessionSummary("fresh", lastActivity = "2026-09-06T17:00:00Z", messageCount = 1)
        val groups = groupSessionsByPersona(listOf(old, fresh), "")
        assertEquals(listOf("fresh", "old"), groups.single().sessions.map { it.id })
        assertEquals(LocalDate.of(2026, 9, 7), sessionHistoryDate(fresh, zone))
        assertEquals(LocalDate.of(2026, 9, 6), sessionHistoryDate(old, zone))
    }

    @Test fun searchMatchesTitlePersonaAndIdIgnoringCaseAndOuterWhitespace() {
        val sessions = listOf(
            SessionSummary("one", name = "Project Chat", messageCount = 2),
            SessionSummary("two", persona = "RESEARCH", messageCount = 2),
            SessionSummary("THREE", messageCount = 2),
        )
        for ((term, id) in listOf(" project " to "one", "research" to "two", "three" to "THREE")) {
            assertEquals(id, groupSessionsByPersona(sessions, term).single().sessions.single().id)
        }
        assertTrue(groupSessionsByPersona(sessions, "missing").isEmpty())
    }

    @Test fun invalidActivityFallsBackToCreationAndUnknownDatesComeLast() {
        val unknown = SessionSummary("unknown", lastActivity = "invalid", messageCount = 1)
        val dated = SessionSummary("dated", createdAt = "2026-09-07T00:00:00Z", messageCount = 1)
        val groups = groupSessionsByPersona(listOf(unknown, dated), "")
        assertEquals("dated", groups.first().sessions.first().id)
        assertEquals("unknown", groups.single().sessions.last().id)
        assertNull(sessionHistoryDate(unknown, zone))
    }

    @Test fun loadingStateDoesNotInitiallyClaimHistoryIsEmpty() {
        assertTrue(SessionsUiState().isLoading)
    }

    @Test fun emptySessionsStayOutOfHistoryEvenWhenNamedOrBoundToPersona() {
        val sessions = listOf(
            SessionSummary("empty"),
            SessionSummary("named", name = "Project Chat", createdAt = "2026-09-07T00:00:00Z"),
            SessionSummary("persona", persona = "RESEARCH"),
        )
        for (query in listOf("", "Project", "RESEARCH", "empty")) {
            assertTrue(groupSessionsByPersona(sessions, query).isEmpty())
        }
    }

    @Test fun pinsAppearInGlobalSectionAndAreNotDuplicatedInPersonaGroups() {
        val defaultSession = SessionSummary("default", lastActivity = "2026-09-07T00:00:00Z", messageCount = 1)
        val researchOld = SessionSummary("research-old", persona = "Research", lastActivity = "2026-09-08T00:00:00Z", messageCount = 1)
        val researchPinned = SessionSummary("research-pinned", persona = "Research", lastActivity = "2026-09-01T00:00:00Z", messageCount = 1)
        val history = buildSessionHistory(
            listOf(defaultSession, researchOld, researchPinned), "", setOf("research-pinned", "default"),
        )
        assertEquals(listOf("default", "research-pinned"), history.pinned.map { it.id })
        assertEquals(listOf("research-old"), history.groups.flatMap { it.sessions }.map { it.id })
        assertEquals(2, history.groups.sumOf { it.pinnedCount })
        assertTrue(history.groups.single { it.persona == null }.sessions.isEmpty())
    }

    @Test fun personaGroupingKeepsSearchAndEmptySessionsRules() {
        val groups = groupSessionsByPersona(
            listOf(
                SessionSummary("empty", persona = "Research"),
                SessionSummary("one", persona = "Research", name = "Project", messageCount = 1),
            ),
            "project",
        )
        assertEquals(listOf("one"), groups.single().sessions.map { it.id })
    }

    @Test fun unpinRestoresActivityOrderAndPinsDoNotBypassSearchOrEmptyFiltering() {
        val recent = SessionSummary("recent", persona = "Research", lastActivity = "2026-09-08T00:00:00Z", messageCount = 1)
        val old = SessionSummary("old", persona = "Research", lastActivity = "2026-09-01T00:00:00Z", messageCount = 1)
        val sessions = listOf(old, recent, SessionSummary("draft", persona = "Research"))
        val pins = setOf("old", "draft", "deleted")
        assertEquals(listOf("old"), buildSessionHistory(sessions, "", pins).pinned.map { it.id })
        assertEquals(listOf("recent", "old"), groupSessionsByPersona(sessions, "").single().sessions.map { it.id })
        assertEquals(listOf(recent), buildSessionHistory(sessions, "recent", pins).groups.single().sessions)
        assertTrue(buildSessionHistory(sessions, "draft", pins).pinned.isEmpty())
        assertTrue(buildSessionHistory(sessions, "draft", pins).groups.isEmpty())
    }

    @Test fun defaultAndMissingPersonaDefinitionsRetainHistoryWithDistinctKeys() {
        val groups = groupSessionsByPersona(
            listOf(
                SessionSummary("default-session", messageCount = 1),
                SessionSummary("empty-persona", persona = "", messageCount = 1),
                SessionSummary("blank-persona", persona = "  ", messageCount = 1),
                SessionSummary("named-default", persona = "default", messageCount = 1),
                SessionSummary("deleted-persona", persona = "Removed persona", messageCount = 1),
            ), "",
        )
        assertEquals(3, groups.size)
        assertEquals(3, groups.first { it.persona == null }.sessions.size)
        assertEquals(groups.size, groups.map { it.key }.toSet().size)
        assertEquals("deleted-persona", groups.first { it.persona == "Removed persona" }.sessions.single().id)
    }

    @Test fun multiplePinsAndGroupsHaveDeterministicOrdering() {
        val sessions = listOf(
            SessionSummary("b", persona = "Other", messageCount = 1),
            SessionSummary("a", persona = "Research", messageCount = 1),
            SessionSummary("recent", persona = "Research", lastActivity = "2026-09-08T00:00:00Z", messageCount = 1),
        )
        val pins = setOf("a", "b")
        val history = buildSessionHistory(sessions, "", pins)
        assertEquals(history, buildSessionHistory(sessions.reversed(), "", pins))
        assertEquals(listOf("a", "b"), history.pinned.map { it.id })
        assertEquals(listOf("Research", "Other"), history.groups.map { it.persona })
        assertEquals(listOf("recent"), history.groups.first().sessions.map { it.id })
    }

    @Test fun firstPersistedMessageMakesSessionVisibleWithoutWaitingForReply() {
        val session = SessionSummary("first", persona = "RESEARCH")
        assertTrue(groupSessionsByPersona(listOf(session), "").isEmpty())
        val sent = session.copy(messageCount = 1)
        val unused = SessionSummary("unused", name = "Unused draft")
        assertEquals(listOf(sent), groupSessionsByPersona(listOf(unused, sent), "").single().sessions)
    }
    @Test fun personaDeletionIncludesPinsSearchHiddenAndEmptySessionsButNotOtherPersonas() {
        val sessions = listOf(
            SessionSummary("pin", persona = "Research", messageCount = 1),
            SessionSummary("hidden", persona = "Research", messageCount = 1),
            SessionSummary("empty", persona = "Research"),
            SessionSummary("other", persona = "Other", messageCount = 1),
            SessionSummary("default", messageCount = 1),
        )
        val history = buildSessionHistory(sessions, "pin", setOf("pin"))
        assertEquals(listOf("pin"), history.pinned.map { it.id })
        assertTrue(history.groups.single().sessions.isEmpty())
        assertEquals(listOf("pin", "hidden", "empty"), personaSessionIds(sessions, history.groups.single().persona))
    }

    @Test fun defaultDeletionNormalizesBlankBindingsAndDoesNotTargetNamedDefault() {
        val sessions = listOf(
            SessionSummary("a"), SessionSummary("b", persona = ""),
            SessionSummary("c", persona = "  "), SessionSummary("d", persona = "default"),
        )
        assertEquals(listOf("a", "b", "c"), personaSessionIds(sessions, null))
        assertEquals(listOf("d"), personaSessionIds(sessions, "default"))
    }

}
