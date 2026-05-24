package dev.clawseed.demo

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Binder
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import dev.clawseed.demo.scheduled.TaskStatus
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.embedded.EmbeddedGateway
import dev.clawseed.sdk.embedded.GatewayState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.withContext

class ClawseedService : Service() {

    inner class LocalBinder : Binder() {
        val service: ClawseedService get() = this@ClawseedService
    }

    private val binder = LocalBinder()
    private val supervisorJob = SupervisorJob()
    private val scope = CoroutineScope(Dispatchers.IO + supervisorJob)

    val gateway = EmbeddedGateway(this)
    val state: StateFlow<GatewayState> = gateway.state

    private val readyCallbacks = mutableListOf<() -> Unit>()
    private var isReady = false

    @Volatile private var isBound = false
    private var everStartedInteractively = false
    @Volatile private var taskChannel = Channel<String>(Channel.UNLIMITED)
    private val unbindSignal = Channel<Unit>(capacity = 1)
    @Volatile private var serviceJob: Job? = null
    @Volatile private var gatewayFailed = false

    override fun onBind(intent: Intent): IBinder {
        isBound = true
        return binder
    }

    override fun onUnbind(intent: Intent?): Boolean {
        isBound = false
        unbindSignal.trySend(Unit)
        return true
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        startForeground(NOTIFICATION_ID, buildNotification("启动 clawseed gateway..."))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val taskId = intent?.getStringExtra(EXTRA_TASK_ID)

        if (taskId != null) {
            if (gatewayFailed) {
                scope.launch { failAndNotify(taskId, "Gateway 启动失败") }
            } else {
                val result = taskChannel.trySend(taskId)
                if (result.isClosed) {
                    scope.launch { failAndNotify(taskId, "Gateway 启动失败") }
                } else {
                    // Wake consumer loop in case it's suspended after calling stopSelf()
                    unbindSignal.trySend(Unit)
                }
            }
        } else {
            everStartedInteractively = true
        }

        if (serviceJob?.isActive != true) {
            gatewayFailed = false
            serviceJob = scope.launch {
                try {
                    // Register flows before starting so they're available during Starting state
                    ClawSeedAndroid.setDownloadProgress(gateway.downloadProgress)
                    gateway.start()
                    val gwState = gateway.state.value
                    if (gwState is GatewayState.Running) {
                        ClawSeedAndroid.init(
                            this@ClawseedService,
                            gateway.localConfig(),
                        )
                        ClawSeedAndroid.setGatewayRestarter { gateway.restart() }
                        isReady = true
                        withContext(Dispatchers.Main) {
                            readyCallbacks.forEach { it() }
                            readyCallbacks.clear()
                        }
                        updateNotification("Gateway 运行中 :${gwState.port}")
                        taskConsumerLoop()
                    } else {
                        updateNotification("启动失败")
                        gatewayFailed = true
                        val failedChannel = taskChannel
                        taskChannel = Channel(Channel.UNLIMITED)
                        failedChannel.close()
                        drainClosedChannel(failedChannel)
                        if (!isBound) stopSelf()
                    }
                } finally {
                    serviceJob = null
                }
            }
        }

        return if (everStartedInteractively) START_STICKY else START_NOT_STICKY
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceJob?.cancel()
        supervisorJob.cancel()
        scope.launch { gateway.stop() }
        isReady = false
    }

    fun onReady(callback: () -> Unit) {
        if (isReady) callback() else readyCallbacks.add(callback)
    }

    fun isGatewayRunning(): Boolean = gateway.state.value is GatewayState.Running

    @OptIn(ExperimentalCoroutinesApi::class)
    private suspend fun taskConsumerLoop() {
        while (true) {
            select<Unit> {
                taskChannel.onReceive { taskId ->
                    executeScheduledTask(taskId)
                }
                unbindSignal.onReceive {
                    // Woken by unbind, fall through to shutdown check
                }
            }
            if (!isBound && taskChannel.isEmpty) {
                // Signal intent to stop, but don't exit the loop.
                // A task might arrive between isEmpty and now; the next
                // select iteration will pick it up. The system will
                // destroy the service via onDestroy() which cancels
                // this coroutine when it's safe to do so.
                stopSelf()
            }
        }
    }

    private suspend fun executeScheduledTask(taskId: String) {
        val store = ScheduledTaskStore(this)
        val task = store.tasksAsList().find { it.id == taskId }
        if (task == null || !task.enabled) return

        updateNotification("正在执行: ${task.name}")

        // Ensure session ID exists — generate one if not set
        var sessionId = task.sessionId
        if (sessionId.isNullOrBlank()) {
            sessionId = java.util.UUID.randomUUID().toString()
            store.updateTaskById(taskId) { it.copy(sessionId = sessionId) }
        }

        // Use WebSocket session so remote tools (device_info, get_location, etc.) are available
        val finalSessionId = sessionId
        try {
            val config = gateway.localConfig()
            val session = dev.clawseed.sdk.core.ClawSeed.createSession(config)

            // Register remote tools for this service session
            registerServiceTools(session)

            // Connect with the session ID (independent of the shared SessionManager)
            session.connect(finalSessionId)

            // Bridge CETP external tools
            runCatching { ClawSeedAndroid.externalToolBridge().attachToRegistry(session.tools) }

            // Wait for connection
            val connected = kotlinx.coroutines.withTimeoutOrNull(10_000L) {
                while (session.connectionState.value != dev.clawseed.sdk.core.model.ConnectionState.CONNECTED) {
                    kotlinx.coroutines.delay(200)
                }
                true
            } ?: false

            if (!connected) {
                showTaskErrorNotification(task, "连接超时", finalSessionId)
                store.updateTaskById(taskId) { current ->
                    current.copy(
                        lastRunAt = System.currentTimeMillis(),
                        lastStatus = TaskStatus.FAILED,
                        lastError = "连接超时",
                    )
                }
                ScheduledTaskManager.onTaskFired(this, taskId)
                return
            }

            // Collect response and title from events
            val responseBuilder = StringBuilder()
            var sessionTitle: String? = null
            val responseJob = scope.launch {
                session.events.collect { event ->
                    when (event) {
                        is dev.clawseed.sdk.core.model.ChatEvent.Done -> {
                            responseBuilder.clear()
                            responseBuilder.append(event.fullResponse)
                        }
                        is dev.clawseed.sdk.core.model.ChatEvent.TitleUpdated -> {
                            sessionTitle = event.title
                        }
                        is dev.clawseed.sdk.core.model.ChatEvent.Error -> {
                            responseBuilder.clear()
                            responseBuilder.append("Error: ${event.message}")
                        }
                        else -> {}
                    }
                }
            }

            // Send message
            session.sendMessage(task.message)

            // Wait for Done event (up to 5 minutes)
            val done = kotlinx.coroutines.withTimeoutOrNull(300_000L) {
                while (responseBuilder.isEmpty()) {
                    kotlinx.coroutines.delay(500)
                }
                true
            } ?: false

            // Wait a bit longer for title generation to complete
            if (done) {
                kotlinx.coroutines.delay(3000)
            }

            responseJob.cancel()
            session.disconnect()

            val response = responseBuilder.toString()
            if (response.isNotEmpty() && !response.startsWith("Error:")) {
                val actualSessionId = session.sessionInfo.value?.sessionId ?: finalSessionId
                showTaskResultNotification(task, response, actualSessionId)
                store.updateTaskById(taskId) { current ->
                    current.copy(
                        lastRunAt = System.currentTimeMillis(),
                        lastStatus = TaskStatus.SUCCESS,
                        lastResult = response.take(500),
                        lastError = null,
                    )
                }
            } else {
                showTaskErrorNotification(task, response.ifBlank { "超时无响应" }, finalSessionId)
                store.updateTaskById(taskId) { current ->
                    current.copy(
                        lastRunAt = System.currentTimeMillis(),
                        lastStatus = TaskStatus.FAILED,
                        lastResult = null,
                        lastError = response.ifBlank { "超时无响应" }.take(200),
                    )
                }
            }
        } catch (e: Exception) {
            showTaskErrorNotification(task, e.message ?: "执行失败", finalSessionId)
            store.updateTaskById(taskId) { current ->
                current.copy(
                    lastRunAt = System.currentTimeMillis(),
                    lastStatus = TaskStatus.FAILED,
                    lastResult = null,
                    lastError = e.message?.take(200),
                )
            }
        }

        ScheduledTaskManager.onTaskFired(this, taskId)
    }

    private fun registerServiceTools(session: dev.clawseed.sdk.core.ClawSeedSession) {
        session.tools.register(
            name = "device_info",
            description = "获取Android设备信息，包括型号、制造商、Android版本",
            parameters = """{"type":"object","properties":{},"required":[]}""",
        ) { _ ->
            val info = kotlinx.serialization.json.buildJsonObject {
                put("model", kotlinx.serialization.json.JsonPrimitive(android.os.Build.MODEL))
                put("manufacturer", kotlinx.serialization.json.JsonPrimitive(android.os.Build.MANUFACTURER))
                put("android_version", kotlinx.serialization.json.JsonPrimitive(android.os.Build.VERSION.RELEASE))
                put("sdk_int", kotlinx.serialization.json.JsonPrimitive(android.os.Build.VERSION.SDK_INT))
            }
            dev.clawseed.sdk.core.tool.ToolResult.Success(info.toString())
        }

        session.tools.register(
            name = "get_location",
            description = "获取用户当前的地理位置信息",
            parameters = """{"type":"object","properties":{},"required":[]}""",
        ) { _ ->
            handleGetLocation()
        }

        // Bridge CETP external tools
        runCatching { ClawSeedAndroid.externalToolBridge().attachToRegistry(session.tools) }
    }

    private fun handleGetLocation(): dev.clawseed.sdk.core.tool.ToolResult {
        val hasPermission = ContextCompat.checkSelfPermission(
            this, Manifest.permission.ACCESS_FINE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED || ContextCompat.checkSelfPermission(
            this, Manifest.permission.ACCESS_COARSE_LOCATION
        ) == PackageManager.PERMISSION_GRANTED

        if (!hasPermission) {
            return dev.clawseed.sdk.core.tool.ToolResult.Failure("位置权限未授予")
        }

        val locationManager = getSystemService(Context.LOCATION_SERVICE) as LocationManager
        val providers = listOf(
            LocationManager.GPS_PROVIDER,
            LocationManager.NETWORK_PROVIDER,
            LocationManager.PASSIVE_PROVIDER,
        )

        var bestLocation: Location? = null
        for (provider in providers) {
            if (!locationManager.isProviderEnabled(provider)) continue
            val loc = try { locationManager.getLastKnownLocation(provider) } catch (_: Exception) { null }
            if (loc != null && (bestLocation == null || loc.time > bestLocation.time)) {
                bestLocation = loc
            }
        }

        if (bestLocation == null) {
            return dev.clawseed.sdk.core.tool.ToolResult.Failure("无法获取位置信息，请确保GPS或网络定位已开启")
        }

        val gcj02 = CoordinateConverter.wgs84ToGcj02(bestLocation.latitude, bestLocation.longitude)
        val result = kotlinx.serialization.json.buildJsonObject {
            put("latitude", kotlinx.serialization.json.JsonPrimitive(gcj02.latitude))
            put("longitude", kotlinx.serialization.json.JsonPrimitive(gcj02.longitude))
            put("accuracy_meters", kotlinx.serialization.json.JsonPrimitive(bestLocation.accuracy.toDouble()))
            put("provider", kotlinx.serialization.json.JsonPrimitive(bestLocation.provider ?: ""))
        }

        try {
            val geocoder = Geocoder(this, java.util.Locale.getDefault())
            @Suppress("DEPRECATION")
            val addresses = geocoder.getFromLocation(bestLocation.latitude, bestLocation.longitude, 1)
            if (!addresses.isNullOrEmpty()) {
                val addr = addresses[0]
                val merged = kotlinx.serialization.json.buildJsonObject {
                    result.forEach { (k, v) -> put(k, v) }
                    addr.locality?.let { put("city", kotlinx.serialization.json.JsonPrimitive(it)) }
                    addr.adminArea?.let { put("province", kotlinx.serialization.json.JsonPrimitive(it)) }
                    addr.subLocality?.let { put("district", kotlinx.serialization.json.JsonPrimitive(it)) }
                    addr.getAddressLine(0)?.let { put("address", kotlinx.serialization.json.JsonPrimitive(it)) }
                }
                return dev.clawseed.sdk.core.tool.ToolResult.Success(merged.toString())
            }
        } catch (_: Exception) {}

        return dev.clawseed.sdk.core.tool.ToolResult.Success(result.toString())
    }

    private suspend fun drainClosedChannel(channel: Channel<String>) {
        for (taskId in channel) {
            failAndNotify(taskId, "Gateway 启动失败")
        }
    }

    private suspend fun failAndNotify(taskId: String, error: String) {
        val store = ScheduledTaskStore(this@ClawseedService)
        val task = store.tasksAsList().find { it.id == taskId }
        if (task != null && task.enabled) {
            store.updateTaskById(taskId) { current ->
                current.copy(
                    lastRunAt = System.currentTimeMillis(),
                    lastStatus = TaskStatus.FAILED,
                    lastError = error,
                )
            }
            showTaskErrorNotification(task, error)
        }
        ScheduledTaskManager.onTaskFired(this@ClawseedService, taskId)
    }

    private fun showTaskResultNotification(task: ScheduledTask, result: String, sessionId: String?) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(MainActivity.EXTRA_SESSION_ID, sessionId)
        }
        val pendingIntent = PendingIntent.getActivity(
            this, task.id.hashCode(), intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(this, CHANNEL_ID_TASKS_REMINDER)
            .setContentTitle("⏰ ${task.name}")
            .setContentText(result.take(100))
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setStyle(NotificationCompat.BigTextStyle().bigText(result.take(500)))
            .setAutoCancel(true)
            .setDefaults(NotificationCompat.DEFAULT_SOUND or NotificationCompat.DEFAULT_VIBRATE)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setContentIntent(pendingIntent)
            .build()
        nm.notify(task.id.hashCode(), notification)
    }

    private fun showTaskErrorNotification(task: ScheduledTask, error: String, sessionId: String? = null) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(MainActivity.EXTRA_SESSION_ID, sessionId)
        }
        val pendingIntent = PendingIntent.getActivity(
            this, task.id.hashCode(), intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(this, CHANNEL_ID_TASKS_REMINDER)
            .setContentTitle("✗ ${task.name}")
            .setContentText(error.take(100))
            .setSmallIcon(android.R.drawable.ic_dialog_alert)
            .setAutoCancel(true)
            .setDefaults(NotificationCompat.DEFAULT_SOUND or NotificationCompat.DEFAULT_VIBRATE)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setContentIntent(pendingIntent)
            .build()
        nm.notify(task.id.hashCode(), notification)
    }

    private fun createNotificationChannel() {
        val gatewayChannel = NotificationChannel(
            CHANNEL_ID, "ClawSeed Gateway",
            NotificationManager.IMPORTANCE_LOW,
        ).apply { description = "ClawSeed gateway service" }

        val taskNotificationChannel = NotificationChannel(
            CHANNEL_ID_TASKS, "定时任务",
            NotificationManager.IMPORTANCE_LOW,
        ).apply { description = "定时任务执行结果" }

        val reminderChannel = NotificationChannel(
            CHANNEL_ID_TASKS_REMINDER, "任务提醒",
            NotificationManager.IMPORTANCE_HIGH,
        ).apply {
            description = "定时任务提醒"
            enableVibration(true)
        }

        getSystemService(NotificationManager::class.java).run {
            createNotificationChannel(gatewayChannel)
            createNotificationChannel(taskNotificationChannel)
            createNotificationChannel(reminderChannel)
        }
    }

    private fun buildNotification(text: String): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("ClawSeed")
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setOngoing(true)
            .setSilent(true)
            .build()

    private fun updateNotification(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIFICATION_ID, buildNotification(text))
    }

    companion object {
        const val EXTRA_TASK_ID = "task_id"
        private const val CHANNEL_ID = "clawseed_gateway"
        private const val CHANNEL_ID_TASKS = "scheduled_tasks"
        private const val CHANNEL_ID_TASKS_REMINDER = "task_reminders"
        private const val NOTIFICATION_ID = 1001
    }
}
