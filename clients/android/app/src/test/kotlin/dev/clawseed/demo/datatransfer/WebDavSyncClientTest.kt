package dev.clawseed.demo.datatransfer

import java.io.File
import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class WebDavSyncClientTest {
    @Test
    fun uploadRetriesTransientHttpFailureBeforeMovingBackup() = runBlocking {
        MockWebServer().use { server ->
            server.enqueue(MockResponse().setResponseCode(503))
            server.enqueue(MockResponse().setResponseCode(201))
            server.enqueue(MockResponse().setResponseCode(201))
            val archive = File.createTempFile("clawseed-test-", ".zip")
            try {
                archive.writeBytes(ByteArray(128) { it.toByte() })
                val result = WebDavSyncClient().uploadFile(
                    server.url("/").toString(),
                    "clawseed-test.zip",
                    archive,
                    null,
                    null,
                    "test-password",
                )

                assertTrue(result.isSuccess)
                assertEquals(3, server.requestCount)
                assertEquals("PUT", server.takeRequest().method)
                assertEquals("PUT", server.takeRequest().method)
                assertEquals("MOVE", server.takeRequest().method)
            } finally {
                archive.delete()
            }
        }
    }

    @Test
    fun timeoutConfigurationRejectsNonPositiveValues() {
        val error = runCatching { WebDavTimeouts(readSeconds = 0) }.exceptionOrNull()

        assertTrue(error is IllegalArgumentException)
    }

    @Test
    fun connectionTestChecksDirectoryAndWritePermission() = runBlocking {
        MockWebServer().use { server ->
            server.enqueue(MockResponse().setResponseCode(207))
            server.enqueue(MockResponse().setResponseCode(201))
            server.enqueue(MockResponse().setResponseCode(204))

            val result = WebDavSyncClient().testConnection(
                server.url("/").toString(),
                "clawseed",
                null,
                null,
            ).getOrThrow()

            assertTrue(result.passed)
            assertEquals(4, result.checks.size)
            assertEquals("PROPFIND", server.takeRequest().method)
            assertEquals("PUT", server.takeRequest().method)
            assertEquals("DELETE", server.takeRequest().method)
        }
    }

    @Test
    fun connectionTestReportsAuthenticationFailureWithoutWriting() = runBlocking {
        MockWebServer().use { server ->
            server.enqueue(MockResponse().setResponseCode(401))

            val result = WebDavSyncClient().testConnection(
                server.url("/").toString(),
                "clawseed",
                "user",
                "password",
            ).getOrThrow()

            assertTrue(!result.passed)
            assertEquals(3, result.checks.size)
            assertEquals(1, server.requestCount)
        }
    }
}
