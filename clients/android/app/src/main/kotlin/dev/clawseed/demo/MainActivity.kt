package dev.clawseed.demo

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import dev.clawseed.client.ToolCallResult
import dev.clawseed.client.ToolSpec
import dev.clawseed.client.ClawseedClient
import org.json.JSONObject

private enum class ConnState { DISCONNECTED, CONNECTING, CONNECTED }

class MainActivity : ComponentActivity() {

    private var clawseedService: ClawseedService? = null
    private val serviceStatus = mutableStateOf("启动 clawseed...")

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName, binder: IBinder) {
            clawseedService = (binder as ClawseedService.LocalBinder).service
            clawseedService?.onReady {
                serviceStatus.value = "Gateway 就绪"
            }
        }
        override fun onServiceDisconnected(name: ComponentName) {
            clawseedService = null
            serviceStatus.value = "Service 断开"
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val serviceIntent = Intent(this, ClawseedService::class.java)
        ContextCompat.startForegroundService(this, serviceIntent)
        bindService(serviceIntent, serviceConnection, Context.BIND_AUTO_CREATE)

        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    DemoScreen(serviceStatusProvider = { serviceStatus.value })
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        unbindService(serviceConnection)
    }
}

@Composable
fun DemoScreen(serviceStatusProvider: () -> String) {
    var url by remember { mutableStateOf("ws://127.0.0.1:42617/ws/chat") }
    var input by remember { mutableStateOf("告诉我设备信息") }
    var output by remember { mutableStateOf("") }
    var connState by remember { mutableStateOf(ConnState.DISCONNECTED) }
    val clientRef = remember { mutableStateOf<ClawseedClient?>(null) }

    DisposableEffect(Unit) {
        onDispose {
            clientRef.value?.disconnect()
            clientRef.value = null
        }
    }

    val deviceInfoSpec = remember {
        ToolSpec(
            name = "device_info",
            description = "获取Android设备信息，包括型号、制造商、Android版本",
            parameters = """{"type":"object","properties":{},"required":[]}""",
        )
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Surface(
            modifier = Modifier.fillMaxWidth(),
            color = MaterialTheme.colorScheme.secondaryContainer,
            shape = MaterialTheme.shapes.small,
        ) {
            Text(
                text = "🤖 ${serviceStatusProvider()}",
                modifier = Modifier.padding(8.dp),
                style = MaterialTheme.typography.labelMedium,
            )
        }

        OutlinedTextField(
            value = url,
            onValueChange = { url = it },
            label = { Text("Gateway URL") },
            modifier = Modifier.fillMaxWidth(),
            enabled = connState == ConnState.DISCONNECTED,
            singleLine = true,
        )

        Button(
            onClick = {
                when (connState) {
                    ConnState.DISCONNECTED -> {
                        connState = ConnState.CONNECTING
                        output = "[连接中...]\n"
                        val client = ClawseedClient.builder(url)
                            .registerTool(deviceInfoSpec)
                            .toolCallHandler { _ ->
                                val info = JSONObject()
                                    .put("model", Build.MODEL)
                                    .put("manufacturer", Build.MANUFACTURER)
                                    .put("android_version", Build.VERSION.RELEASE)
                                    .put("sdk_int", Build.VERSION.SDK_INT)
                                ToolCallResult.Success(info.toString())
                            }
                            .onConnected { connState = ConnState.CONNECTED; output += "[已连接]\n" }
                            .onDisconnected { connState = ConnState.DISCONNECTED; output += "[已断开]\n" }
                            .onChunk { chunk -> output += chunk }
                            .onDone { _ -> output += "\n✓ 完成\n" }
                            .onError { err -> output += "\n[ERROR] $err\n" }
                            .build()
                        client.connect()
                        clientRef.value = client
                    }
                    ConnState.CONNECTED -> {
                        connState = ConnState.DISCONNECTED
                        clientRef.value?.disconnect()
                        clientRef.value = null
                    }
                    ConnState.CONNECTING -> Unit
                }
            },
            enabled = connState != ConnState.CONNECTING,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(when (connState) {
                ConnState.DISCONNECTED -> "连接"
                ConnState.CONNECTING -> "连接中..."
                ConnState.CONNECTED -> "断开"
            })
        }

        OutlinedTextField(
            value = input,
            onValueChange = { input = it },
            label = { Text("消息") },
            modifier = Modifier.fillMaxWidth(),
        )

        Button(
            onClick = {
                if (input.isNotBlank()) {
                    clientRef.value?.sendMessage(input)
                    output += "\n> $input\n"
                }
            },
            enabled = connState == ConnState.CONNECTED,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("发送")
        }

        val scrollState = rememberScrollState()
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.surfaceVariant,
            shape = MaterialTheme.shapes.medium,
        ) {
            Text(
                text = output,
                modifier = Modifier
                    .padding(12.dp)
                    .verticalScroll(scrollState),
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}
