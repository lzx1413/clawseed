package dev.clawseed.sdk.core.client

import dev.clawseed.sdk.core.model.*
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.*
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import kotlin.test.*

class FileAttachmentsTest {
    @Test fun legacyGatewaysAndMessagesHaveNoFileCapability() {
        assertFalse(Json.decodeFromString<GatewayStatus>("""{"model":"old"}""").fileAttachmentsSupported)
        assertTrue(Json.decodeFromString<SessionMessage>("""{"role":"user","content":"old"}""").files.isEmpty())
        val event = ChatEvent.parse("""{"type":"session_start","session_id":"a","file_attachments_supported":true}""", Json) as ChatEvent.SessionStarted
        assertTrue(event.fileAttachmentsSupported)
        assertFalse(event.imageAttachmentsSupported)
    }

    @Test fun filesOnlyAndMixedMessagesRetainStructuredMetadata() = runBlocking {
        val received = kotlinx.coroutines.channels.Channel<JsonObject>(4)
        val server = MockWebServer()
        server.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                val obj = Json.parseToJsonElement(text).jsonObject
                if (obj["type"]?.jsonPrimitive?.content == "message") received.trySend(obj)
            }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(code, reason) }
        }))
        server.start()
        val client = ChatClient(server.url("/ws/chat").toString().replaceFirst("http://", "ws://"), { null }, ToolRegistry(), ReconnectPolicy.None)
        val id = "file_" + "a".repeat(32)
        val file = FileAttachment(id, "中文 file.md", "text/markdown", 6, "md", unit = "character", total = 2,
            excerpt = AttachmentReadResult(id, "character", 0, 2, 2, true, "文件"))
        try {
            client.connect("session-a")
            client.sendMessage("", files = listOf(file))
            val onlyFile = withTimeout(5000) { received.receive() }
            assertEquals("", onlyFile["content"]!!.jsonPrimitive.content)
            // Inspect the wire JSON: decoding back into Kotlin would hide a missing default.
            assertEquals("ready", onlyFile["files"]!!.jsonArray.single().jsonObject["status"]?.jsonPrimitive?.content)
            assertEquals(file, Json.decodeFromJsonElement<FileAttachment>(onlyFile["files"]!!.jsonArray.single()))
            client.sendMessage("问题", attachments = listOf(ImageAttachment("att_image", "image/png", 4, 1, 1)), files = listOf(file))
            val mixed = withTimeout(5000) { received.receive() }
            assertEquals("image", mixed["attachments"]!!.jsonArray.single().jsonObject["type"]!!.jsonPrimitive.content)
            assertEquals(id, mixed["files"]!!.jsonArray.single().jsonObject["id"]!!.jsonPrimitive.content)
        } finally { client.close(); server.shutdown() }
    }
}
