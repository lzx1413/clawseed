package dev.clawseed.demo.datatransfer

import dev.clawseed.sdk.core.model.UserProfile
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

internal object UserProfileArchive {
    const val ENTRY_NAME = "user_profile/profile.json"

    private val json = Json {
        ignoreUnknownKeys = true
        prettyPrint = true
    }

    fun encode(profile: UserProfile): String = json.encodeToString(profile)

    fun decode(content: String): UserProfile = json.decodeFromString(content)
}
