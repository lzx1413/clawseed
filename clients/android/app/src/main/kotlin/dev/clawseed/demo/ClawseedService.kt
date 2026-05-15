package dev.clawseed.demo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import androidx.core.app.NotificationCompat
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

        val client = ClawSeedAndroid.gatewayClient()
        val result = client.webhook(task.message, task.sessionId)

        result.onSuccess { response ->
            showTaskResultNotification(task, response.response, task.sessionId)
            store.updateTaskById(taskId) { current ->
                current.copy(
                    lastRunAt = System.currentTimeMillis(),
                    lastStatus = TaskStatus.SUCCESS,
                    lastResult = response.response.take(500),
                    lastError = null,
                )
            }
        }.onFailure { error ->
            showTaskErrorNotification(task, error.message ?: "Unknown error")
            store.updateTaskById(taskId) { current ->
                current.copy(
                    lastRunAt = System.currentTimeMillis(),
                    lastStatus = TaskStatus.FAILED,
                    lastResult = null,
                    lastError = error.message?.take(200),
                )
            }
        }

        ScheduledTaskManager.onTaskFired(this, taskId)
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

    private fun showTaskErrorNotification(task: ScheduledTask, error: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val notification = NotificationCompat.Builder(this, CHANNEL_ID_TASKS_REMINDER)
            .setContentTitle("✗ ${task.name}")
            .setContentText(error.take(100))
            .setSmallIcon(android.R.drawable.ic_dialog_alert)
            .setAutoCancel(true)
            .setDefaults(NotificationCompat.DEFAULT_SOUND or NotificationCompat.DEFAULT_VIBRATE)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
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
