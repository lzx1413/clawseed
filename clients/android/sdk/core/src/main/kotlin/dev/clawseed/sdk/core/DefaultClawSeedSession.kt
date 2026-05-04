package dev.clawseed.sdk.core

import dev.clawseed.sdk.core.client.ChatClient
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

internal class DefaultClawSeedSession(
    config: ClawSeedConfig,
) : ClawSeedSession {

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val toolRegistry = ToolRegistry()

    private val chatClient = ChatClient(
        url = buildWsUrl(config.gatewayUrl),
        authTokenProvider = config.authTokenProvider,
        toolRegistry = toolRegistry,
        reconnectPolicy = config.reconnectPolicy,
    )

    override val gateway: GatewayClient = GatewayClient(
        baseUrl = config.gatewayUrl,
        authTokenProvider = config.authTokenProvider,
    )

    private val _sessionInfo = MutableStateFlow<SessionInfo?>(null)
    override val sessionInfo: StateFlow<SessionInfo?> = _sessionInfo.asStateFlow()

    override val connectionState: StateFlow<ConnectionState> = chatClient.connectionState
    override val events: SharedFlow<ChatEvent> = chatClient.events
    override val tools: ToolRegistry = toolRegistry

    init {
        scope.launch {
            events.collect { event ->
                if (event is ChatEvent.SessionStarted) {
                    _sessionInfo.value = SessionInfo(
                        sessionId = event.sessionId,
                        name = event.name,
                        resumed = event.resumed,
                        messageCount = event.messageCount,
                    )
                }
            }
        }
    }

    override suspend fun connect(sessionId: String?) {
        chatClient.connect(sessionId)
    }

    override suspend fun disconnect() {
        chatClient.disconnect()
    }

    override fun sendMessage(content: String, debug: Boolean) {
        chatClient.sendMessage(content, debug)
    }

    override suspend fun abort() {
        // Try WebSocket abort first (lower latency)
        chatClient.sendAbort()
        // Also call REST abort as fallback for reliability
        val sid = _sessionInfo.value?.sessionId ?: return
        runCatching { gateway.abortSession(sid) }
    }

    override fun close() {
        chatClient.disconnect()
    }

    companion object {
        private fun buildWsUrl(httpUrl: String): String {
            val base = httpUrl.trimEnd('/')
            return when {
                base.startsWith("https://") -> base.replace("https://", "wss://") + "/ws/chat"
                base.startsWith("http://") -> base.replace("http://", "ws://") + "/ws/chat"
                else -> "ws://$base/ws/chat"
            }
        }
    }
}
