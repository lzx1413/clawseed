package dev.clawseed.sdk.core

import dev.clawseed.sdk.core.client.ReconnectPolicy

/**
 * Configuration used when creating a [ClawSeedSession].
 */
data class ClawSeedConfig(
    /** Base HTTP gateway URL, for example `http://127.0.0.1:3000`. */
    val gatewayUrl: String,
    /** Supplies the bearer token used by HTTP and WebSocket requests. */
    val authTokenProvider: () -> String? = { null },
    /** Controls automatic reconnection after unexpected disconnects. */
    val reconnectPolicy: ReconnectPolicy = ReconnectPolicy.ExponentialBackoff(),
) {
    /** Convenience constructor for callers that use a fixed bearer token. */
    constructor(
        gatewayUrl: String,
        authToken: String?,
        reconnectPolicy: ReconnectPolicy = ReconnectPolicy.ExponentialBackoff(),
    ) : this(gatewayUrl, { authToken }, reconnectPolicy)
}
