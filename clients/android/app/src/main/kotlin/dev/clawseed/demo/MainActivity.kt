package dev.clawseed.demo

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.os.Build
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.ViewModelProvider
import dev.clawseed.demo.sharing.SharedInbox
import dev.clawseed.demo.sharing.ShareReceiverViewModel
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import dev.clawseed.demo.ui.theme.appColorScheme
import androidx.core.content.ContextCompat
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.i18n.LocaleHelper
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.ScheduledTaskStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private val serviceRef = mutableStateOf<ClawseedService?>(null)
    private lateinit var localStore: LocalStore
    private val pendingSessionId = mutableStateOf<String?>(null)
    private var pendingAlarmDismissId: String? = null
    private var gatewayServiceStarted = false
    private val shareReceiver by lazy { ViewModelProvider(this)[ShareReceiverViewModel::class.java] }
    private var shareRequestId: String? = null

    private val nearbyWifiPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) {
        startGatewayService()
    }

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            serviceRef.value = (binder as ClawseedService.LocalBinder).service
            pendingAlarmDismissId?.let { serviceRef.value?.dismissAlarm(it) }
            pendingAlarmDismissId = null
        }
        override fun onServiceDisconnected(name: ComponentName) {
            serviceRef.value = null
        }
    }

    override fun attachBaseContext(newBase: android.content.Context) {
        super.attachBaseContext(LocaleHelper.wrapContext(newBase))
    }

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        localStore = LocalStore(this)

        if (requiresNearbyWifiPermission() &&
            ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.NEARBY_WIFI_DEVICES,
            ) != PackageManager.PERMISSION_GRANTED
        ) {
            nearbyWifiPermissionLauncher.launch(Manifest.permission.NEARBY_WIFI_DEVICES)
        } else {
            startGatewayService()
        }

        CoroutineScope(Dispatchers.IO).launch {
            ScheduledTaskManager.rescheduleAll(this@MainActivity)
            val store = ScheduledTaskStore(this@MainActivity)
            store.ensureDefaultTasks(this@MainActivity)
            store.resetStuckRunningTasks()
        }

        // Handle session ID from notification tap
        if (SharedInbox.isShare(intent)) {
            shareRequestId = savedInstanceState?.getString("share_request_id") ?: java.util.UUID.randomUUID().toString()
            shareReceiver.receive(checkNotNull(shareRequestId), Intent(intent))
        } else {
            handleIntentSession(intent)
            if (pendingSessionId.value == null) shareReceiver.recover()
        }
        handleAlarmDismiss(intent)

        setContent {
            val shareState by shareReceiver.state.collectAsState()
            LaunchedEffect(shareState.openSession) {
                shareState.openSession?.let {
                    pendingSessionId.value = it
                    shareReceiver.navigationHandled(it)
                }
            }
            val themeMode by localStore.themeMode.collectAsState(initial = "system")
            val oledMode by localStore.oledMode.collectAsState(initial = false)
            val useDarkTheme = when (themeMode) {
                "light" -> false
                "dark" -> true
                else -> isSystemInDarkTheme()
            }
            val colorScheme = appColorScheme(useDarkTheme, oledMode)
            MaterialTheme(colorScheme = colorScheme) {
                Surface(modifier = Modifier.fillMaxSize()) {
                    ClawseedApp(
                        localStore = localStore,
                        notificationSessionId = pendingSessionId,
                    )
                }
                if (shareState.receiving) androidx.compose.material3.AlertDialog(
                    onDismissRequest = {},
                    title = { androidx.compose.material3.Text("正在接收分享") },
                    text = { androidx.compose.material3.Text("正在保存图片和文件，即将打开新会话…") },
                    confirmButton = {},
                )
                shareState.error?.let { error -> androidx.compose.material3.AlertDialog(
                    onDismissRequest = { shareReceiver.dismissError() },
                    title = { androidx.compose.material3.Text("无法接收分享") },
                    text = { androidx.compose.material3.Text(error) },
                    confirmButton = { androidx.compose.material3.TextButton(onClick = { shareReceiver.dismissError() }) { androidx.compose.material3.Text("知道了") } },
                ) }
            }
        }
    }

    override fun onSaveInstanceState(outState: android.os.Bundle) {
        shareRequestId?.let { outState.putString("share_request_id", it) }
        super.onSaveInstanceState(outState)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (SharedInbox.isShare(intent)) {
            shareRequestId = java.util.UUID.randomUUID().toString()
            shareReceiver.receive(checkNotNull(shareRequestId), Intent(intent))
        } else {
            shareRequestId = null
            handleIntentSession(intent)
        }
        handleAlarmDismiss(intent)
    }

    private fun handleAlarmDismiss(intent: Intent?) {
        val taskId = intent?.getStringExtra(EXTRA_ALARM_DISMISS) ?: return
        intent.removeExtra(EXTRA_ALARM_DISMISS)
        val service = serviceRef.value
        if (service != null) service.dismissAlarm(taskId) else pendingAlarmDismissId = taskId
    }

    private fun handleIntentSession(intent: Intent?) {
        val sessionId = intent?.getStringExtra(EXTRA_SESSION_ID)
        if (sessionId != null) {
            pendingSessionId.value = sessionId
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        if (gatewayServiceStarted) {
            unbindService(serviceConnection)
        }
    }

    private fun startGatewayService() {
        if (gatewayServiceStarted) return
        val serviceIntent = Intent(this, ClawseedService::class.java)
        ContextCompat.startForegroundService(this, serviceIntent)
        gatewayServiceStarted = bindService(
            serviceIntent,
            serviceConnection,
            Context.BIND_AUTO_CREATE,
        )
    }

    private fun requiresNearbyWifiPermission(): Boolean = Build.VERSION.SDK_INT == 36

    companion object {
        const val EXTRA_SESSION_ID = "session_id"
        const val EXTRA_ALARM_DISMISS = "alarm_dismiss"
    }
}
