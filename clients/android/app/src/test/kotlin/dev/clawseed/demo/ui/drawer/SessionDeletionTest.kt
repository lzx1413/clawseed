package dev.clawseed.demo.ui.drawer

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test

class SessionDeletionTest {
    @Test fun deletesConfirmedSnapshotOnceAndReportsSuccessImmediately() = runTest {
        val events = mutableListOf<String>()
        val result = deleteHistorySessions(
            listOf("one", "two", "one"),
            delete = { events += "delete:$it"; Result.success(Unit) },
            onDeleted = { events += "removed:$it" },
        )
        assertEquals(listOf("delete:one", "removed:one", "delete:two", "removed:two"), events)
        assertEquals(setOf("one", "two"), result.deleted)
        assertTrue(result.failed.isEmpty())
    }

    @Test fun partialFailureRetainsFailedSessionsAndContinuesRemainingDeletes() = runTest {
        val removed = mutableListOf<String>()
        val result = deleteHistorySessions(
            listOf("good", "failed", "throws", "last"),
            delete = {
                when (it) {
                    "failed" -> Result.failure(IllegalStateException("HTTP error"))
                    "throws" -> throw IllegalStateException("Disconnected")
                    else -> Result.success(Unit)
                }
            },
            onDeleted = { removed += it },
        )
        assertEquals(listOf("good", "last"), removed)
        assertEquals(setOf("good", "last"), result.deleted)
        assertEquals(setOf("failed", "throws"), result.failed)
    }

    @Test fun emptySnapshotMakesNoRequests() = runTest {
        val result = deleteHistorySessions(emptyList(), { error("Unexpected delete") }, { error("Unexpected callback") })
        assertTrue(result.deleted.isEmpty())
        assertTrue(result.failed.isEmpty())
    }

    @Test fun cancellationStopsBatchEvenWhenWrappedBySdk() = runTest {
        for (wrapped in listOf(false, true)) {
            val attempted = mutableListOf<String>()
            val removed = mutableListOf<String>()
            try {
                deleteHistorySessions(
                    listOf("good", "cancel", "untouched"),
                    delete = {
                        attempted += it
                        if (it == "cancel") {
                            val cancellation = CancellationException("Cancelled")
                            if (wrapped) Result.failure(cancellation) else throw cancellation
                        } else Result.success(Unit)
                    },
                    onDeleted = { removed += it },
                )
                fail("Cancellation must propagate")
            } catch (_: CancellationException) {
                assertEquals(listOf("good", "cancel"), attempted)
                assertEquals(listOf("good"), removed)
            }
        }
    }
}
