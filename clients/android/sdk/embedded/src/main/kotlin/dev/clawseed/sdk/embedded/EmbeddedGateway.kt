package dev.clawseed.sdk.embedded

import android.content.Context
import android.util.Log
import dev.clawseed.sdk.core.ClawSeedConfig
import dev.clawseed.sdk.core.model.EmbeddingDownloadProgress
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

    private val _downloadProgress = MutableStateFlow<EmbeddingDownloadProgress?>(null)
    /** Current embedding model download progress, null when not downloading. */
    val downloadProgress: StateFlow<EmbeddingDownloadProgress?> = _downloadProgress.asStateFlow()

    private var process: Process? = null

    /** Starts the embedded gateway and waits until health checks succeed. */
    suspend fun start() {
        if (_state.value is GatewayState.Running) return
        _state.value = GatewayState.Starting

        try {
            // Kill any stale process occupying our port before starting a new one
            ensurePortIsFree()

            val binary = File(context.applicationInfo.nativeLibraryDir, config.binaryName)
            if (!binary.exists()) error("clawseed binary not found: ${binary.absolutePath}")
            Log.i(TAG, "Using binary: ${binary.absolutePath} (${binary.length()} bytes)")

            val configManager = GatewayConfigManager(context)
            configManager.ensureConfig()
            configManager.ensureBundledModelFiles()

            process = ProcessBuilder(binary.absolutePath, "gateway", "--port", config.port.toString())
                .redirectErrorStream(true)
                .also { pb ->
                    val filesDir = context.filesDir
                    pb.environment()["HOME"] = filesDir.absolutePath
                    pb.environment()["XDG_CONFIG_HOME"] = filesDir.absolutePath
                    pb.environment()["XDG_DATA_HOME"] = filesDir.absolutePath
                    pb.environment()["CLAWSEED_GATEWAY_TIMEOUT_SECS"] = "300"
                    // Point ort's load-dynamic to the bundled libonnxruntime.so
                    val nativeLibDir = context.applicationInfo.nativeLibraryDir
                    pb.environment()["ORT_DYLIB_PATH"] = "$nativeLibDir/libonnxruntime.so"
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
                try {
                    process?.inputStream?.bufferedReader()?.forEachLine { line ->
                        Log.d(TAG, "clawseed: $line")
                        parseDownloadMarker(ANSI_REGEX.replace(line, ""))
                    }
                } catch (_: java.io.InterruptedIOException) {
                    // Expected when stop() closes the process inputStream from another thread
                    Log.i(TAG, "Stdout reader interrupted — gateway stopping")
                } catch (_: java.io.IOException) {
                    // Process terminated — stream closed naturally
                    Log.i(TAG, "Stdout reader closed — gateway process ended")
                }
            }

            waitUntilReady()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start clawseed gateway", e)
            _state.value = GatewayState.Failed(e.message ?: "Unknown error")
            _downloadProgress.value = null
        }
    }

    /** Stops the embedded gateway process if it is running.
     *  Waits for the process to fully exit and the port to be released before returning. */
    suspend fun stop() {
        val proc = process
        if (proc != null) {
            proc.destroy()
            withContext(Dispatchers.IO) {
                // Wait up to 3s for clean exit, then force-kill
                val exited = proc.waitFor(3, java.util.concurrent.TimeUnit.SECONDS)
                if (!exited) {
                    proc.destroyForcibly()
                    proc.waitFor(1, java.util.concurrent.TimeUnit.SECONDS)
                }
            }
        }
        process = null
        _state.value = GatewayState.Stopped
        _downloadProgress.value = null
    }

    /** Restarts the gateway: stops the current process, waits for port to be free, then starts fresh. */
    suspend fun restart() {
        stop()
        ensurePortIsFree()
        start()
    }

    /** Checks that no stale process is serving on our port. If found, waits for it to stop. */
    private suspend fun ensurePortIsFree() {
        val healthUrl = "http://127.0.0.1:${config.port}/health"
        // Check if a stale process is still responding on our port
        var staleDetected = false
        var attempts = 30 // 15 seconds max wait for stale process to die
        while (attempts > 0) {
            val code = withContext(Dispatchers.IO) {
                try {
                    val conn = URL(healthUrl).openConnection() as HttpURLConnection
                    conn.connectTimeout = 300
                    conn.readTimeout = 300
                    val result = conn.responseCode
                    conn.disconnect()
                    result
                } catch (_: Exception) { -1 }
            }
            if (code < 0) {
                // Port is free (connection refused) — we're good
                if (staleDetected) Log.i(TAG, "Stale gateway on port ${config.port} has stopped")
                return
            }
            staleDetected = true
            Log.w(TAG, "Stale gateway still on port ${config.port}, waiting... (attempts left: $attempts)")
            delay(500)
            attempts--
        }
        Log.e(TAG, "Could not free port ${config.port} — stale process persists")
    }

    /** Returns a local [ClawSeedConfig] targeting the embedded gateway port. */
    fun localConfig(): ClawSeedConfig = ClawSeedConfig(
        gatewayUrl = "http://127.0.0.1:${config.port}",
    )

    private suspend fun waitUntilReady() {
        val maxAttempts = (config.healthCheckTimeoutMs / config.healthCheckIntervalMs).toInt()
        val healthUrl = "http://127.0.0.1:${config.port}/health"

        // Phase 1: wait for port to become unavailable briefly (ensures we don't connect
        // to a stale process from a previous run). Skip if port is already free.
        var portWasUnavailable = false
        for (i in 1..20) {
            val code = withContext(Dispatchers.IO) {
                try {
                    val conn = URL(healthUrl).openConnection() as HttpURLConnection
                    conn.connectTimeout = 200
                    conn.readTimeout = 200
                    val result = conn.responseCode
                    conn.disconnect()
                    result
                } catch (_: Exception) { -1 }
            }
            if (code >= 0) {
                // Port is responding — a stale process might still be there
                Log.w(TAG, "Health endpoint responded during startup phase 1, attempt $i")
                delay(500)
            } else {
                portWasUnavailable = true
                break
            }
        }
        if (!portWasUnavailable) {
            // Port stayed responsive — something is very wrong. Kill our process and fail.
            process?.destroy()
            withContext(Dispatchers.IO) { process?.waitFor(2, java.util.concurrent.TimeUnit.SECONDS) }
            process = null
            _state.value = GatewayState.Failed("Port ${config.port} is occupied by a stale process")
            _downloadProgress.value = null
            return
        }

        // Phase 2: wait for our new process to become ready
        var attempts = 0
        while (attempts < maxAttempts) {
            val code = withContext(Dispatchers.IO) {
                try {
                    val conn = URL(healthUrl).openConnection() as HttpURLConnection
                    conn.connectTimeout = 500
                    conn.readTimeout = 500
                    val result = conn.responseCode
                    conn.disconnect()
                    result
                } catch (_: Exception) { -1 }
            }
            if (code in 200..299) {
                _state.value = GatewayState.Running(config.port)
                _downloadProgress.value = null
                Log.i(TAG, "clawseed gateway ready on port ${config.port}")
                return
            }
            delay(config.healthCheckIntervalMs)
            attempts++
        }
        // Kill the process that failed to become ready
        process?.destroy()
        withContext(Dispatchers.IO) {
            process?.waitFor(2, java.util.concurrent.TimeUnit.SECONDS)
        }
        process = null
        _state.value = GatewayState.Failed("Gateway did not become ready in time")
        _downloadProgress.value = null
        Log.w(TAG, "Gateway did not become ready in time")
    }

    companion object {
        private const val TAG = "EmbeddedGateway"
        private val ANSI_REGEX = Regex("\\[[0-9;]*m")
        private const val PROGRESS_PREFIX = "EMBEDDING_DOWNLOAD_PROGRESS:"
        private const val START_PREFIX = "EMBEDDING_DOWNLOAD_START:"
        private const val COMPLETE_PREFIX = "EMBEDDING_DOWNLOAD_COMPLETE:"
    }

    private fun parseDownloadMarker(cleanLine: String) {
        when {
            cleanLine.contains(PROGRESS_PREFIX) -> {
                val payload = cleanLine.substringAfter(PROGRESS_PREFIX)
                val parts = payload.split(":")
                if (parts.size >= 4) {
                    val percentRaw = parts[0].toIntOrNull() ?: 0
                    val downloadedBytes = parts[1].toLongOrNull() ?: 0L
                    val totalBytesStr = parts[2]
                    val filename = parts[3]
                    val totalBytes = if (totalBytesStr == "-1") null else totalBytesStr.toLongOrNull()
                    val percent = if (totalBytes != null && totalBytes > 0) {
                        (downloadedBytes * 100 / totalBytes).toInt()
                    } else null
                    _downloadProgress.value = EmbeddingDownloadProgress(
                        filename = filename,
                        downloadedBytes = downloadedBytes,
                        totalBytes = totalBytes,
                        percent = percent,
                        isComplete = false,
                    )
                }
            }
            cleanLine.contains(COMPLETE_PREFIX) -> {
                val payload = cleanLine.substringAfter(COMPLETE_PREFIX)
                val parts = payload.split(":")
                if (parts.size >= 2) {
                    val filename = parts[0]
                    val totalBytes = parts[1].toLongOrNull()
                    _downloadProgress.value = EmbeddingDownloadProgress(
                        filename = filename,
                        downloadedBytes = totalBytes ?: 0L,
                        totalBytes = totalBytes,
                        percent = 100,
                        isComplete = true,
                    )
                }
            }
            cleanLine.contains(START_PREFIX) -> {
                val payload = cleanLine.substringAfter(START_PREFIX)
                val parts = payload.split(":")
                if (parts.size >= 2) {
                    val filename = parts[0]
                    val totalBytesStr = parts[1]
                    val totalBytes = if (totalBytesStr == "-1") null else totalBytesStr.toLongOrNull()
                    _downloadProgress.value = EmbeddingDownloadProgress(
                        filename = filename,
                        downloadedBytes = 0L,
                        totalBytes = totalBytes,
                        percent = 0,
                        isComplete = false,
                    )
                }
            }
        }
    }
}
