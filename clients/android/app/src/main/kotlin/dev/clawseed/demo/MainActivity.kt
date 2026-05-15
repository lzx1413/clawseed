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
                    background = Color.Black,
                    onBackground = Color.White,
                    surface = Color.Black,
                    onSurface = Color.White,
                    surfaceVariant = Color(0xFF1C1C1C),
                    onSurfaceVariant = Color(0xFFBFBFBF),
                )
                useDarkTheme -> darkColorScheme()
                else -> lightColorScheme()
            }
            MaterialTheme(colorScheme = colorScheme) {
                Surface(modifier = Modifier.fillMaxSize()) {
                    ClawseedApp(
                        localStore = localStore,
                    )
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        unbindService(serviceConnection)
    }
}
