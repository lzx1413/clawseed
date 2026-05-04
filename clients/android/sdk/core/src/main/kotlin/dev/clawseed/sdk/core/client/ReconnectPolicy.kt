package dev.clawseed.sdk.core.client

/** Controls automatic reconnection after unexpected WebSocket disconnects. */
sealed class ReconnectPolicy {
    /** Disables automatic reconnection. */
    data object None : ReconnectPolicy()

    /** Reconnects with exponential backoff and bounded retry delay. */
    data class ExponentialBackoff(
        /** Delay used before the first reconnect attempt. */
        val initialDelayMs: Long = 1000,
        /** Maximum delay allowed between reconnect attempts. */
        val maxDelayMs: Long = 30_000,
        /** Maximum number of reconnect attempts before giving up. */
        val maxAttempts: Int = Int.MAX_VALUE,
    ) : ReconnectPolicy()
}
