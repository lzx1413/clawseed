package dev.clawseed.demo

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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

        val serviceIntent = Intent(this, ClawseedService::class.java)
        ContextCompat.startForegroundService(this, serviceIntent)
        bindService(serviceIntent, serviceConnection, Context.BIND_AUTO_CREATE)

        CoroutineScope(Dispatchers.IO).launch {
            ScheduledTaskManager.rescheduleAll(this@MainActivity)
            val store = ScheduledTaskStore(this@MainActivity)
            store.ensureDefaultTasks(this@MainActivity)
            store.resetStuckRunningTasks()
        }

        // Handle session ID from notification tap
        handleIntentSession(intent)
        handleAlarmDismiss(intent)

        setContent {
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
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntentSession(intent)
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
        unbindService(serviceConnection)
    }

    companion object {
        const val EXTRA_SESSION_ID = "session_id"
        const val EXTRA_ALARM_DISMISS = "alarm_dismiss"
    }
}
