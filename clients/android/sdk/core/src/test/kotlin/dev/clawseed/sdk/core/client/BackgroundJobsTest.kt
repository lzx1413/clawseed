package dev.clawseed.sdk.core.client

import kotlinx.coroutines.test.runTest
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class BackgroundJobsTest {
    @Test fun authenticatesPollsAndRejectsUnavailableOrMalformedCounts() = runTest {
        MockWebServer().use { server ->
            server.start()
            val client = GatewayClient(server.url("").toString().trimEnd('/'), "local-token")
            server.enqueue(MockResponse().setBody("""{"active_count":2,"jobs":[]}"""))
            assertEquals(2, client.activeBackgroundJobCount().getOrThrow())
            val request = server.takeRequest()
            assertEquals("/api/background-jobs", request.path)
            assertEquals("Bearer local-token", request.getHeader("Authorization"))
            for (body in listOf("{}", """{"active_count":-1}""", """{"active_count":"invalid"}""")) {
                server.enqueue(MockResponse().setBody(body))
                assertTrue(client.activeBackgroundJobCount().isFailure)
            }
            server.enqueue(MockResponse().setResponseCode(503))
            assertTrue(client.activeBackgroundJobCount().isFailure)
            server.enqueue(MockResponse().setBody("""{"active_count":0}"""))
            assertEquals(0, client.activeBackgroundJobCount().getOrThrow())
        }
    }
}
