package dev.clawseed.demo

import org.junit.Assert.*
import org.junit.Test

class ExecutionKeepAliveTest {
    @Test fun retainsJobsAfterChatAndIgnoresAStaleIdlePoll() {
        val state = ExecutionKeepAlive(0)
        state.generationChanged(1, 0)
        val stale = state.pollRevision()
        state.generationChanged(0, 10)
        state.polled(stale, 0, 20)
        assertTrue(state.shouldKeepAlive(20))
        state.polled(state.pollRevision(), 1, 30)
        assertTrue(state.hasBackgroundCommands())
        assertTrue(state.shouldKeepAlive(30))
        state.polled(state.pollRevision(), 0, 40)
        assertFalse(state.shouldKeepAlive(40))
    }

    @Test fun transientErrorsPreserveABoundedLeaseAndChatRemainsAuthoritative() {
        val state = ExecutionKeepAlive(0)
        state.polled(state.pollRevision(), 2, 100)
        state.polled(state.pollRevision(), null, 1_000)
        assertTrue(state.shouldKeepAlive(1_000))
        state.polled(state.pollRevision(), null, 100 + ExecutionKeepAlive.LEASE_MS)
        assertFalse(state.shouldKeepAlive(100 + ExecutionKeepAlive.LEASE_MS))
        state.generationChanged(1, 200 + ExecutionKeepAlive.LEASE_MS)
        state.polled(state.pollRevision(), 0, 201 + ExecutionKeepAlive.LEASE_MS)
        assertTrue(state.shouldKeepAlive(201 + ExecutionKeepAlive.LEASE_MS))
    }
}
