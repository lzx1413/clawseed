package dev.clawseed.demo.ui.drawer

import dev.clawseed.sdk.core.model.SessionSummary
import java.time.LocalDate
import java.time.ZoneId
import org.junit.Assert.*
import org.junit.Test

class SessionHistoryTest {
    private val zone = ZoneId.of("Asia/Shanghai")

    @Test fun sortsByActivityAndGroupsInLocalTimezone() {
        val old = SessionSummary("old", lastActivity = "2026-09-06T15:00:00Z")
        val fresh = SessionSummary("fresh", lastActivity = "2026-09-06T17:00:00Z")
        val groups = groupSessionHistory(listOf(old, fresh), "", zone)
        assertEquals(listOf(LocalDate.of(2026, 9, 7), LocalDate.of(2026, 9, 6)), groups.map { it.date })
        assertEquals("fresh", groups.first().sessions.single().id)
    }

    @Test fun searchMatchesTitlePersonaAndIdIgnoringCaseAndOuterWhitespace() {
        val sessions = listOf(
            SessionSummary("one", name = "Project Chat"),
            SessionSummary("two", persona = "RESEARCH"),
            SessionSummary("THREE"),
        )
        for ((term, id) in listOf(" project " to "one", "research" to "two", "three" to "THREE")) {
            assertEquals(id, groupSessionHistory(sessions, term, zone).single().sessions.single().id)
        }
        assertTrue(groupSessionHistory(sessions, "missing", zone).isEmpty())
    }

    @Test fun invalidActivityFallsBackToCreationAndUnknownDatesComeLast() {
        val unknown = SessionSummary("unknown", lastActivity = "invalid")
        val dated = SessionSummary("dated", createdAt = "2026-09-07T00:00:00Z")
        val groups = groupSessionHistory(listOf(unknown, dated), "", zone)
        assertEquals("dated", groups.first().sessions.single().id)
        assertNull(groups.last().date)
    }

    @Test fun loadingStateDoesNotInitiallyClaimHistoryIsEmpty() {
        assertTrue(SessionsUiState().isLoading)
    }
}
