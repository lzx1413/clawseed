package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class MemoryEntry(
    val id: String,
    val key: String,
    val content: String,
    val category: String,
    val timestamp: String,
    @SerialName("session_id") val sessionId: String? = null,
    val score: Double? = null,
    val namespace: String = "default",
    val importance: Double? = null,
    @SerialName("superseded_by") val supersededBy: String? = null,
)

@Serializable
data class MemoryEntries(val entries: List<MemoryEntry> = emptyList())
