package dev.clawseed.demo.datatransfer

import dev.clawseed.sdk.core.model.ProfileCategory
import dev.clawseed.sdk.core.model.ProfileSource
import dev.clawseed.sdk.core.model.ProfileStatus
import dev.clawseed.sdk.core.model.UserProfile
import dev.clawseed.sdk.core.model.UserProfileItem
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class UserProfileArchiveTest {
    @Test
    fun profileSnapshotRoundTripPreservesAuditFields() {
        val profile = UserProfile(
            userId = "owner",
            version = 7,
            items = listOf(
                UserProfileItem(
                    id = "item-1",
                    userId = "owner",
                    key = "preference.response_style",
                    value = JsonPrimitive("concise"),
                    category = ProfileCategory.PREFERENCE,
                    confidence = 0.87,
                    source = ProfileSource.INFERRED,
                    status = ProfileStatus.REJECTED,
                    evidenceSessionId = "session-1",
                    expiresAt = "2027-01-01T00:00:00Z",
                    createdAt = "2026-07-16T00:00:00Z",
                    updatedAt = "2026-07-16T01:00:00Z",
                    version = 7,
                ),
            ),
        )

        val restored = UserProfileArchive.decode(UserProfileArchive.encode(profile))

        assertEquals(profile, restored)
        assertEquals("user_profile/profile.json", UserProfileArchive.ENTRY_NAME)
    }
}
