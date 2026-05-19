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
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.core.content.ContextCompat
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private val serviceRef = mutableStateOf<ClawseedService?>(null)
    private lateinit var localStore: LocalStore
    private val pendingSessionId = mutableStateOf<String?>(null)

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            serviceRef.value = (binder as ClawseedService.LocalBinder).service
        }
        override fun onServiceDisconnected(name: ComponentName) {
            serviceRef.value = null
        }
    }

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        localStore = LocalStore(this)

        val serviceIntent = Intent(this, ClawseedService::class.java)
        ContextCompat.startForegroundService(this, serviceIntent)
        bindService(serviceIntent, serviceConnection, Context.BIND_AUTO_CREATE)

        CoroutineScope(Dispatchers.IO).launch {
            ScheduledTaskManager.rescheduleAll(this@MainActivity)
        }

        // Handle session ID from notification tap
        handleIntentSession(intent)

        setContent {
            val themeMode by localStore.themeMode.collectAsState(initial = "system")
            val oledMode by localStore.oledMode.collectAsState(initial = false)
            val useDarkTheme = when (themeMode) {
                "light" -> false
                "dark" -> true
                else -> isSystemInDarkTheme()
            }
            val colorScheme = when {
                useDarkTheme && oledMode -> darkColorScheme(
                    primary = Color(0xFFE8A44A),
                    onPrimary = Color(0xFF432800),
                    primaryContainer = Color(0xFF5F3A00),
                    onPrimaryContainer = Color(0xFFFFDDB3),
                    secondary = Color(0xFFD4914A),
                    onSecondary = Color(0xFF3B1E00),
                    background = Color.Black,
                    onBackground = Color.White,
                    surface = Color.Black,
                    onSurface = Color.White,
                    surfaceVariant = Color(0xFF1C1C1C),
                    onSurfaceVariant = Color(0xFFBFBFBF),
                )
                useDarkTheme -> darkColorScheme(
                    primary = Color(0xFFE8A44A),
                    onPrimary = Color(0xFF432800),
                    primaryContainer = Color(0xFF5F3A00),
                    onPrimaryContainer = Color(0xFFFFDDB3),
                    secondary = Color(0xFFD4914A),
                    onSecondary = Color(0xFF3B1E00),
                )
                else -> lightColorScheme(
                    primary = Color(0xFFB0722A),
                    onPrimary = Color(0xFFFFFFFF),
                    primaryContainer = Color(0xFFFFDDB3),
                    onPrimaryContainer = Color(0xFF3B1E00),
                    secondary = Color(0xFF9A6324),
                    onSecondary = Color(0xFFFFFFFF),
                )
            }
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
    }
}
