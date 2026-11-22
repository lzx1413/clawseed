package dev.clawseed.demo

/** A chat can finish before its background commands. Ignore poll results from
 * an earlier generation state, and preserve the lease during transient failures. */
internal class ExecutionKeepAlive(nowMs: Long) {
    private var generationCount = 0
    private var revision = 0L
    private var awaitingPoll = true
    private var deadline = nowMs + 15_000L
    private var backgroundCount = 0

    @Synchronized fun generationChanged(count: Int, nowMs: Long) {
        if (count != generationCount) {
            generationCount = count
            revision++
            awaitingPoll = true
            deadline = nowMs + LEASE_MS
        }
    }

    @Synchronized fun pollRevision(): Long = revision

    @Synchronized fun polled(pollRevision: Long, count: Int?, nowMs: Long) {
        if (pollRevision != revision) return
        if (count != null) {
            backgroundCount = count
            awaitingPoll = false
            if (count > 0) deadline = nowMs + LEASE_MS
        } else if (nowMs >= deadline) {
            backgroundCount = 0
            awaitingPoll = false
        }
    }

    @Synchronized fun shouldKeepAlive(nowMs: Long): Boolean =
        generationCount > 0 || ((backgroundCount > 0 || awaitingPoll) && nowMs < deadline)

    @Synchronized fun hasBackgroundCommands(): Boolean = backgroundCount > 0

    companion object {
        // Background commands have a one-hour ceiling; tolerate a short cleanup margin.
        const val LEASE_MS = 65 * 60 * 1000L
    }
}
