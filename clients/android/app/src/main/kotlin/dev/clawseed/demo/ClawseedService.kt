package dev.clawseed.demo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Binder
import android.os.IBinder
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.ClawSeedConfig
import dev.clawseed.sdk.embedded.EmbeddedGateway
import dev.clawseed.sdk.embedded.GatewayState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

class ClawseedService : Service() {

    inner class LocalBinder : Binder() {
        val service: ClawseedService get() = this@ClawseedService
    }

    private val binder = LocalBinder()
    private val supervisorJob = SupervisorJob()
    private val scope = CoroutineScope(Dispatchers.IO + supervisorJob)
    private var serviceJob: Job? = null

    val gateway = EmbeddedGateway(this)
    val state: StateFlow<GatewayState> = gateway.state

    private val readyCallbacks = mutableListOf<() -> Unit>()
    private var isReady = false

    override fun onBind(intent: Intent): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        startForeground(NOTIFICATION_ID, buildNotification("启动 clawseed gateway..."))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (serviceJob == null) {
            serviceJob = scope.launch {
                gateway.start()
                val gwState = gateway.state.value
                if (gwState is GatewayState.Running) {
                    ClawSeedAndroid.init(
                        this@ClawseedService,
                        gateway.localConfig(),
                    )
                    isReady = true
                    updateNotification("Gateway 运行中 :${gwState.port}")
                    readyCallbacks.forEach { it() }
                    readyCallbacks.clear()
                } else {
                    updateNotification("启动失败")
                }
            }
        }
        return START_STICKY
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

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID, "ClawSeed Gateway",
            NotificationManager.IMPORTANCE_LOW,
        ).apply { description = "ClawSeed gateway service" }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    private fun buildNotification(text: String): Notification =
        androidx.core.app.NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("ClawSeed")
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setOngoing(true)
            .build()

    private fun updateNotification(text: String) {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.notify(NOTIFICATION_ID, buildNotification(text))
    }

    companion object {
        private const val CHANNEL_ID = "clawseed_gateway"
        private const val NOTIFICATION_ID = 1001
    }
}
