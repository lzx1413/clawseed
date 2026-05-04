package dev.clawseed.sdk.core.client

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
}