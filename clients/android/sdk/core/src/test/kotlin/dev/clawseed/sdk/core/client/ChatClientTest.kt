package dev.clawseed.sdk.core.client

import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ChatClientTest {

    @Test
    fun resolveSessionIdKeepsExistingSessionWhenReconnectOmitsId() {
        assertEquals(
            "session-1",
            ChatClient.resolveSessionId(requestedSessionId = null, currentSessionId = "session-1"),
        )
    }

    @Test
    fun resolveSessionIdPrefersExplicitSessionId() {
        assertEquals(
            "session-2",
            ChatClient.resolveSessionId(requestedSessionId = "session-2", currentSessionId = "session-1"),
        )
    }

    @Test
    fun shouldReconnectOnCloseForUnexpectedNonNormalClose() {
        assertTrue(ChatClient.shouldReconnectOnClose(code = 1001, intentionalDisconnect = false))
    }

    @Test
    fun shouldNotReconnectOnNormalOrIntentionalClose() {
        assertFalse(ChatClient.shouldReconnectOnClose(code = 1000, intentionalDisconnect = false))
        assertFalse(ChatClient.shouldReconnectOnClose(code = 1001, intentionalDisconnect = true))
    }

    @Test
    fun websocketChunksArePublishedInArrivalOrder() = runBlocking {
        val chunkCount = 1_000
        val server = MockWebServer()
        server.enqueue(
            MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    repeat(chunkCount) { index ->
                        webSocket.send("""{"type":"chunk","content":"$index,"}""")
                    }
                }
            }),
        )
        server.start()

        val client = ChatClient(
            url = server.url("/api/chat/ws").toString().replaceFirst("http://", "ws://"),
            authTokenProvider = { null },
            toolRegistry = ToolRegistry(),
            reconnectPolicy = ReconnectPolicy.None,
        )
        val received = async(start = CoroutineStart.UNDISPATCHED) {
            withTimeout(10_000) {
                client.events
                    .filterIsInstance<ChatEvent.TextChunk>()
                    .take(chunkCount)
                    .map { it.content }
                    .toList()
            }
        }

        try {
            client.connect()
            assertEquals(List(chunkCount) { "$it," }, received.await())
        } finally {
            client.disconnect()
            server.shutdown()
        }
    }
}
