package dev.clawseed.demo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

class ClawseedService : Service() {

    inner class LocalBinder : Binder() {
        val service: ClawseedService get() = this@ClawseedService
    }

    private val binder = LocalBinder()
    private val supervisorJob = SupervisorJob()
    private val scope = CoroutineScope(Dispatchers.IO + supervisorJob)

    private var process: Process? = null
    private var serviceJob: Job? = null

    private val readyCallbacks = mutableListOf<() -> Unit>()
    private var isReady = false

    override fun onBind(intent: Intent): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        startForeground(NOTIFICATION_ID, buildNotification("启动 clawseed gateway..."))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (process == null) {
            serviceJob = scope.launch { startGateway() }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceJob?.cancel()
        supervisorJob.cancel()
        process?.destroy()
        process = null
        isReady = false
    }

    fun onReady(callback: () -> Unit) {
        if (isReady) callback() else readyCallbacks.add(callback)
    }

    fun isGatewayRunning(): Boolean = process?.isAlive == true

    private suspend fun startGateway() {
        try {
            val binary = File(applicationInfo.nativeLibraryDir, "libclawseed.so")
            if (!binary.exists()) error("clawseed binary not found: ${binary.absolutePath}")
            Log.i(TAG, "Using binary: ${binary.absolutePath} (${binary.length()} bytes)")

            ensureConfig()
            updateNotification("Gateway 运行中 :42617")

            process = ProcessBuilder(binary.absolutePath, "gateway", "--port", "42617")
                .redirectErrorStream(true)
                .also { pb ->
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
            updateNotification("启动失败: ${e.message}")
        }
    }

    private fun ensureConfig() {
        val configDir = File(filesDir, ".clawseed")
        configDir.mkdirs()

        // Ensure workspace directory exists so file tools can read/write
        val workspaceDir = File(configDir, "workspace")
        if (!workspaceDir.exists()) {
            workspaceDir.mkdirs()
            Log.i(TAG, "Created workspace directory: ${workspaceDir.absolutePath}")
        }

        val configFile = File(configDir, "config.toml")
        if (configFile.exists()) {
            var content = configFile.readText()
            var changed = false

            // Ensure workspace_dir is set in config
            if (!content.contains("workspace_dir")) {
                content = "workspace_dir = \"${workspaceDir.absolutePath}\"\n$content"
                changed = true
                Log.i(TAG, "Added workspace_dir to config")
            }

            for ((section, patch) in WEB_FEATURE_PATCHES) {
                val patched = enableSectionIfPresent(content, section, patch)
                if (patched != content) {
                    content = patched
                    changed = true
                }
            }

            if (changed) configFile.writeText(content)
        } else {
            configFile.writeText(INITIAL_CONFIG.replace("{WORKSPACE_DIR}", workspaceDir.absolutePath))
            Log.i(TAG, "Created initial config")
        }
    }

    private fun enableSectionIfPresent(content: String, sectionHeader: String, patch: Pair<String, String>): String {
        val sectionIdx = content.indexOf("\n$sectionHeader\n")
        if (sectionIdx == -1) return content
        val nextSection = content.indexOf("\n[", sectionIdx + 1).let { if (it == -1) content.length else it }
        val before = content.substring(0, sectionIdx)
        var section = content.substring(sectionIdx, nextSection)
        val after = content.substring(nextSection)
        if (section.contains(patch.first)) {
            section = section.replace(patch.first, patch.second)
            Log.i(TAG, "Patched config: $sectionHeader ${patch.second}")
        }
        // Ensure allowed_domains is present for network tool sections
        if (sectionHeader in listOf("[http_request]", "[web_fetch]") && !section.contains("allowed_domains")) {
            section = section.trimEnd() + "\nallowed_domains = [\"*\"]\n"
            Log.i(TAG, "Added allowed_domains to $sectionHeader")
        }
        return before + section + after
    }

    companion object {
        private const val TAG = "ClawseedService"
        private const val CHANNEL_ID = "clawseed_gateway"
        private const val NOTIFICATION_ID = 1001
        private const val MAX_HEALTH_ATTEMPTS = 40
        private const val HEALTH_POLL_MS = 500L

        private val WEB_FEATURE_PATCHES = listOf(
            "[web_fetch]"    to ("enabled = false" to "enabled = true"),
            "[http_request]" to ("enabled = false" to "enabled = true"),
            "[web_search]"   to ("enabled = false" to "enabled = true"),
        )

        private val INITIAL_CONFIG = """
workspace_dir = "{WORKSPACE_DIR}"

[gateway]

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "duckduckgo"
""".trimIndent() + "\n"
    }

    private suspend fun waitUntilReady() {
        val healthUrl = "http://127.0.0.1:42617/health"
        repeat(MAX_HEALTH_ATTEMPTS) {
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
                isReady = true
                Log.i(TAG, "clawseed gateway ready")
                updateNotification("Gateway 已就绪 :42617")
                withContext(Dispatchers.Main) {
                    readyCallbacks.forEach { it() }
                    readyCallbacks.clear()
                }
                return
            }
            delay(HEALTH_POLL_MS)
        }
        Log.w(TAG, "Gateway did not become ready in time")
        updateNotification("Gateway 无响应 — 查看 logcat")
        stopSelf()
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID, "ClawSeed Gateway",
            NotificationManager.IMPORTANCE_LOW,
        ).apply { description = "ClawSeed gateway service" }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun buildNotification(text: String): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("ClawSeed")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setOngoing(true)
            .build()

    private fun updateNotification(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIFICATION_ID, buildNotification(text))
    }
}
