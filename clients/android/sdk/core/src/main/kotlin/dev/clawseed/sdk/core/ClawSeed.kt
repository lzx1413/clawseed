package dev.clawseed.sdk.core

/**
 * Entry point for creating ClawSeed SDK sessions.
 *
 * The returned [ClawSeedSession] is configured but not connected. Call
 * [ClawSeedSession.connect] before sending messages.
 */
object ClawSeed {
    /** Creates a new SDK session backed by [config]. */
    fun createSession(config: ClawSeedConfig): ClawSeedSession =
        DefaultClawSeedSession(config)
}
