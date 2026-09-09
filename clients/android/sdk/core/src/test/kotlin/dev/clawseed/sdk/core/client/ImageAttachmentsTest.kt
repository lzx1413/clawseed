package dev.clawseed.sdk.core.client

import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.GatewayStatus
import dev.clawseed.sdk.core.model.SessionMessage
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.*
import org.junit.Test

class ImageAttachmentsTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test fun legacyHistoryAndGatewayDefaultToNoImages() {
        assertTrue(json.decodeFromString<SessionMessage>("""{"role":"user","content":"hello"}""").attachments.isEmpty())
        assertFalse(json.decodeFromString<GatewayStatus>("""{"model":"text-model"}""").imageAttachments.supported)
    }

    @Test fun imageContextNotificationListsOmittedImages() {
        val event = ChatEvent.parse("""{"type":"image_context","omitted_ids":["att_old"]}""", json) as ChatEvent.ImageContext
        assertEquals(listOf("att_old"), event.omittedIds)
    }

    @Test fun uploadAndReadUseSameAuthenticationAndSessionScope() = runBlocking {
        val server = MockWebServer()
        server.start()
        try {
            val metadata = """{"id":"att_image","mime_type":"image/png","size_bytes":3,"width":1,"height":1}"""
            server.enqueue(MockResponse().setResponseCode(201).setBody(metadata))
            server.enqueue(MockResponse().setBody(okio.Buffer().write(byteArrayOf(1, 2, 3))))
            val gateway = GatewayClient(server.url("/prefix").toString(), "private-test-token")
            val image = gateway.uploadImage("session/a", byteArrayOf(1, 2, 3)).getOrThrow()
            val upload = server.takeRequest()
            assertEquals("/prefix/api/sessions/session%2Fa/attachments", upload.path)
            assertEquals("Bearer private-test-token", upload.getHeader("Authorization"))
            assertArrayEquals(byteArrayOf(1, 2, 3), upload.body.readByteArray())
            assertArrayEquals(byteArrayOf(1, 2, 3), gateway.readImage("session/a", image.id).getOrThrow())
            val read = server.takeRequest()
            assertEquals("/prefix/api/sessions/session%2Fa/attachments/att_image", read.path)
            assertEquals("Bearer private-test-token", read.getHeader("Authorization"))
        } finally { server.shutdown() }
    }
}
