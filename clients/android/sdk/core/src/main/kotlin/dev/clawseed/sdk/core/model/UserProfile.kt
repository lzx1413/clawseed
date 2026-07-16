package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

@Serializable
enum class ProfileCategory {
    @SerialName("identity")
    IDENTITY,

    @SerialName("preference")
    PREFERENCE,

    @SerialName("expertise")
    EXPERTISE,

    @SerialName("goal")
    GOAL,

    @SerialName("constraint")
    CONSTRAINT,

    @SerialName("accessibility")
    ACCESSIBILITY,
}

@Serializable
enum class ProfileSource {
    @SerialName("explicit")
    EXPLICIT,

    @SerialName("inferred")
    INFERRED,

    @SerialName("imported")
    IMPORTED,
}

@Serializable
enum class ProfileStatus {
    @SerialName("active")
    ACTIVE,

    @SerialName("superseded")
    SUPERSEDED,

    @SerialName("rejected")
    REJECTED,
}

@Serializable
enum class ProfileImportStrategy {
    @SerialName("replace")
    REPLACE,

    @SerialName("merge")
    MERGE,

    @SerialName("append")
    APPEND,
}

@Serializable
data class UserProfileItem(
    val id: String,
    @SerialName("user_id") val userId: String,
    val key: String,
    val value: JsonElement,
    val category: ProfileCategory,
    val confidence: Double,
    val source: ProfileSource,
    val status: ProfileStatus,
    @SerialName("evidence_session_id") val evidenceSessionId: String? = null,
    @SerialName("expires_at") val expiresAt: String? = null,
    @SerialName("created_at") val createdAt: String,
    @SerialName("updated_at") val updatedAt: String,
    val version: Long,
)

@Serializable
data class UserProfile(
    @SerialName("user_id") val userId: String,
    val version: Long,
    val items: List<UserProfileItem> = emptyList(),
)

@Serializable
data class UserProfileUpsert(
    val key: String,
    val value: JsonElement,
    val category: ProfileCategory,
    @SerialName("expires_at") val expiresAt: String? = null,
)

@Serializable
data class UserProfilePatch(
    val value: JsonElement? = null,
    val category: ProfileCategory? = null,
    val status: ProfileStatus? = null,
    @SerialName("expires_at") val expiresAt: String? = null,
    @SerialName("clear_expires_at") val clearExpiresAt: Boolean = false,
)

@Serializable
data class UserProfileImportItem(
    val key: String,
    val value: JsonElement,
    val category: ProfileCategory,
    val confidence: Double,
    val status: ProfileStatus,
    @SerialName("evidence_session_id") val evidenceSessionId: String? = null,
    @SerialName("expires_at") val expiresAt: String? = null,
) {
    companion object {
        fun from(item: UserProfileItem): UserProfileImportItem = UserProfileImportItem(
            key = item.key,
            value = item.value,
            category = item.category,
            confidence = item.confidence,
            status = item.status,
            evidenceSessionId = item.evidenceSessionId,
            expiresAt = item.expiresAt,
        )
    }
}

@Serializable
data class UserProfileImportRequest(
    val strategy: ProfileImportStrategy,
    val items: List<UserProfileImportItem>,
)

@Serializable
data class UserProfileImportResult(
    val imported: Int,
    val skipped: Int,
)
