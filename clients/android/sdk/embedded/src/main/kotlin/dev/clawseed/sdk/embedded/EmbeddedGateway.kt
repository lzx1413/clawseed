package dev.clawseed.sdk.embedded

import android.content.Context
import android.util.Log
import dev.clawseed.sdk.core.ClawSeedConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** Starts and monitors an embedded ClawSeed gateway process on Android. */
class EmbeddedGateway(
    private val context: Context,
    private val config: EmbeddedGatewayConfig = EmbeddedGatewayConfig(),
) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val _state = MutableStateFlow<GatewayState>(GatewayState.Stopped)
    /** Current runtime state of the embedded gateway. */
    val state: StateFlow<GatewayState> = _state.asStateFlow()

    private var process: Process? = null

    /** Starts the embedded gateway and waits until health checks succeed. */
    suspend fun start() {
        if (_state.value is GatewayState.Running) return
        _state.value = GatewayState.Starting

        try {
            val binary = File(context.applicationInfo.nativeLibraryDir, config.binaryName)
            if (!binary.exists()) error("clawseed binary not found: ${binary.absolutePath}")
            Log.i(TAG, "Using binary: ${binary.absolutePath} (${binary.length()} bytes)")

            val configManager = GatewayConfigManager(context)
            configManager.ensureConfig()

            process = ProcessBuilder(binary.absolutePath, "gateway", "--port", config.port.toString())
                .redirectErrorStream(true)
                .also { pb ->
                    val filesDir = context.filesDir
                    pb.environment()["HOME"] = filesDir.absolutePath
                    pb.environment()["XDG_CONFIG_HOME"] = filesDir.absolutePath
                    pb.environment()["XDG_DATA_HOME"] = filesDir.absolutePath
                    val apiKeyFile = File(filesDir, ".clawseed/api_key")
                    if (apiKeyFile.exists()) {
                        val key = apiKeyFile.readText().trim()
                        if (key.isNotEmpty()) {
                            pb.environment()["CLAWSEED_API_KEY"] = key
                            Log.i(TAG, "API key loaded from file")
                        }
                    }
                }
                .start()

            scope.launch {
                process?.inputStream?.bufferedReader()?.forEachLine { line ->
                    Log.d(TAG, "clawseed: $line")
                }
            }

            waitUntilReady()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start clawseed gateway", e)
            _state.value = GatewayState.Failed(e.message ?: "Unknown error")
        }
    }

    /** Stops the embedded gateway process if it is running. */
    suspend fun stop() {
        process?.destroy()
        process = null
        _state.value = GatewayState.Stopped
    }

    /** Restarts the gateway by calling [stop] then [start]. */
    suspend fun restart() {
        stop()
        start()
    }

    /** Returns a local [ClawSeedConfig] targeting the embedded gateway port. */
    fun localConfig(): ClawSeedConfig = ClawSeedConfig(
        gatewayUrl = "http://127.0.0.1:${config.port}",
    )

    private suspend fun waitUntilReady() {
        val maxAttempts = (config.healthCheckTimeoutMs / config.healthCheckIntervalMs).toInt()
        val healthUrl = "http://127.0.0.1:${config.port}/health"
        repeat(maxAttempts) {
            val code = withContext(Dispatchers.IO) {
                try {
                    val conn = URL(healthUrl).openConnection() as HttpURLConnection
                    conn.connectTimeout = 500
                    conn.readTimeout = 500
                    val result = conn.responseCode
                    conn.disconnect()
                    result
                } catch (_: Exception) {
                    -1
                }
            }
            if (code in 200..299) {
                _state.value = GatewayState.Running(config.port)
                Log.i(TAG, "clawseed gateway ready on port ${config.port}")
                return
            }
            delay(config.healthCheckIntervalMs)
        }
        _state.value = GatewayState.Failed("Gateway did not become ready in time")
        Log.w(TAG, "Gateway did not become ready in time")
    }

    companion object {
        private const val TAG = "EmbeddedGateway"
    }
}
