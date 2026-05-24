package dev.clawseed.sdk.embedded

/** Configuration for starting an embedded on-device gateway process. */
data class EmbeddedGatewayConfig(
    /** Local TCP port used by the embedded gateway. */
    val port: Int = 42617,
    /** Executable file name expected inside `nativeLibraryDir`. */
    val binaryName: String = "libclawseed.so",
    /** Maximum time to wait for the health endpoint to come up.
     *  300s allows for model download on first startup with local embedding (~80MB). */
    val healthCheckTimeoutMs: Long = 300_000,
    /** Delay between health checks while starting the process. */
    val healthCheckIntervalMs: Long = 500,
)
