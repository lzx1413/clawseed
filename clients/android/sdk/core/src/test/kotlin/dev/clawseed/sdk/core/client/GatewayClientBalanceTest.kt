package dev.clawseed.sdk.core.client

import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class GatewayClientBalanceTest {
    @Test
    fun usesAuthenticatedProxyAndKeepsDraftKeyOutOfUrl() = runTest {
        MockWebServer().use { server ->
            server.start()
            server.enqueue(MockResponse().setBody("""{"status":"available","balances":[{"currency":"CNY","available":"0.00000"}]}"""))
            val client = GatewayClient(server.url("").toString().trimEnd('/'), "gateway-token")
            val balance = client.providerBalance("https://api.deepseek.com/v1", "draft-secret").getOrThrow()
            assertEquals("0.00000", balance.balances.single().available)
            val request = server.takeRequest()
            assertEquals("/api/provider/balance", request.path)
            assertEquals("Bearer gateway-token", request.getHeader("Authorization"))
            val body = Json.parseToJsonElement(request.body.readUtf8()).jsonObject
            assertEquals("draft-secret", body["api_key"]?.jsonPrimitive?.content)

            server.enqueue(MockResponse().setBody("""{"status":"permission_denied","balances":[]}"""))
            assertEquals("permission_denied", client.providerBalance("https://openrouter.ai/api/v1", null).getOrThrow().status)
            assertFalse(Json.parseToJsonElement(server.takeRequest().body.readUtf8()).jsonObject.containsKey("api_key"))

            server.enqueue(MockResponse().setResponseCode(502).setBody("secret upstream data"))
            val failure = client.providerBalance("https://api.deepseek.com/v1", "draft-secret")
            assertTrue(failure.isFailure)
            assertEquals("HTTP 502", failure.exceptionOrNull()?.message)
        }
    }
}
