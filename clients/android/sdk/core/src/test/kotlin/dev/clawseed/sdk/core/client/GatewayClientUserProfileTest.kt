package dev.clawseed.sdk.core.client

import dev.clawseed.sdk.core.model.ProfileCategory
import dev.clawseed.sdk.core.model.ProfileStatus
import dev.clawseed.sdk.core.model.UserProfilePatch
import dev.clawseed.sdk.core.model.UserProfileUpsert
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class GatewayClientUserProfileTest {
    private val itemJson = """
        {
          "id":"item-1","user_id":"owner","key":"preference.language","value":"zh-CN",
          "category":"preference","confidence":0.95,"source":"inferred","status":"active",
          "evidence_session_id":"session-1","expires_at":null,
          "created_at":"2026-07-16T00:00:00Z","updated_at":"2026-07-16T00:00:00Z","version":1
        }
    """.trimIndent()

    @Test
    fun readsAndCreatesProfileItemsWithAuthentication() = runTest {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody("""{"user_id":"owner","version":1,"items":[$itemJson]}"""))
            server.enqueue(MockResponse().setResponseCode(201).setBody(itemJson))
            val client = GatewayClient(server.url("").toString().trimEnd('/'), "token-1")

            val profile = client.userProfile().getOrThrow()
            assertEquals("preference.language", profile.items.single().key)
            assertEquals("zh-CN", profile.items.single().value.toString().trim('"'))
            val getRequest = server.takeRequest()
            assertEquals("/api/users/me/profile", getRequest.path)
            assertEquals("Bearer token-1", getRequest.getHeader("Authorization"))

            client.upsertUserProfileItem(
                UserProfileUpsert(
                    key = "preference.language",
                    value = JsonPrimitive("zh-CN"),
                    category = ProfileCategory.PREFERENCE,
                ),
            ).getOrThrow()
            val postRequest = server.takeRequest()
            assertEquals("POST", postRequest.method)
            val payload = Json.parseToJsonElement(postRequest.body.readUtf8()).toString()
            assertTrue(payload.contains("\"category\":\"preference\""))
        } finally {
            server.shutdown()
        }
    }

    @Test
    fun rejectsDeletesAndClearsProfileItems() = runTest {
        val server = MockWebServer()
        server.start()
        try {
            server.enqueue(MockResponse().setBody(itemJson.replace("\"active\"", "\"rejected\"")))
            server.enqueue(MockResponse().setBody("""{"deleted":true}"""))
            server.enqueue(MockResponse().setBody("""{"deleted":3}"""))
            val client = GatewayClient(server.url("").toString().trimEnd('/'), null)

            client.patchUserProfileItem(
                "item-1",
                UserProfilePatch(status = ProfileStatus.REJECTED),
            ).getOrThrow()
            val patchRequest = server.takeRequest()
            assertEquals("PATCH", patchRequest.method)
            assertEquals("/api/users/me/profile/items/item-1", patchRequest.path)
            assertTrue(patchRequest.body.readUtf8().contains("\"status\":\"rejected\""))

            client.deleteUserProfileItem("item-1").getOrThrow()
            assertEquals("DELETE", server.takeRequest().method)
            assertEquals(3, client.clearUserProfile().getOrThrow())
            assertEquals("/api/users/me/profile", server.takeRequest().path)
        } finally {
            server.shutdown()
        }
    }
}
