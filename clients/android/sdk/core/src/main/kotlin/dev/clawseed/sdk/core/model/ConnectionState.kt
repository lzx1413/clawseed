package dev.clawseed.sdk.core.model

/** Connection state for the chat transport managed by [dev.clawseed.sdk.core.ClawSeedSession]. */
enum class ConnectionState {
    /** No active connection exists. */
    DISCONNECTED,
    /** A connection attempt is in progress. */
    CONNECTING,
    /** The session is connected and can exchange messages. */
    CONNECTED,
    /** The SDK is retrying after an unexpected disconnect. */
    RECONNECTING,
}
