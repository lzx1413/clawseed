package dev.clawseed.sdk.embedded

/** State of the embedded gateway process managed on device. */
sealed class GatewayState {
    /** No gateway process is running. */
    data object Stopped : GatewayState()
    /** Gateway process is starting and health checks are in progress. */
    data object Starting : GatewayState()
    /** Gateway is healthy and listening on [port]. */
    data class Running(val port: Int) : GatewayState()
    /** Gateway startup failed with [error]. */
    data class Failed(val error: String) : GatewayState()
}
