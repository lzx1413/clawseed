package dev.clawseed.demo.ui.profile

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UserProfileValueCodecTest {
    @Test
    fun stringValuesAreDisplayedWithoutJsonQuotes() {
        assertEquals("concise", UserProfileValueCodec.display(JsonPrimitive("concise")))
        assertEquals("42", UserProfileValueCodec.display(JsonPrimitive(42)))
    }

    @Test
    fun validJsonIsParsedAndPlainTextRemainsAString() {
        assertTrue(UserProfileValueCodec.parse("[\"zh\", \"en\"]") is JsonArray)
        assertEquals(JsonPrimitive("plain text"), UserProfileValueCodec.parse("plain text"))
        assertEquals(JsonPrimitive(""), UserProfileValueCodec.parse("   "))
    }

    @Test
    fun profileKeysMatchGatewayValidation() {
        assertTrue(isUserProfileKeyValid("preference.response_style"))
        assertTrue(isUserProfileKeyValid("goal-2026_07"))
        assertFalse(isUserProfileKeyValid("preference/response"))
        assertFalse(isUserProfileKeyValid("偏好.语言"))
        assertFalse(isUserProfileKeyValid(""))
        assertFalse(isUserProfileKeyValid("a".repeat(257)))
    }
}
