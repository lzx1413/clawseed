package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Snapshot of gateway runtime status and provider configuration. */
@Serializable
data class GatewayStatus(
    val provider: String? = null,
    val model: String = "",
    val temperature: Double = 0.7,
    /** Legacy field — replaced by `memory` object. */
    @SerialName("memory_backend") val memoryBackend: String? = null,
    val paired: Boolean = false,
    @SerialName("gateway_port") val gatewayPort: Int = 0,
    val memory: MemoryStatus? = null,
)

/** Memory subsystem status from the gateway. */
@Serializable
data class MemoryStatus(
    val backend: String = "none",
    @SerialName("embedding_provider") val embeddingProvider: String = "none",
    @SerialName("embedding_model") val embeddingModel: String = "",
    @SerialName("embedding_dims") val embeddingDims: Int = 0,
    @SerialName("search_mode") val searchMode: String = "hybrid",
    val count: Int = 0,
)

/** Description of one tool exposed by the gateway. */
@Serializable
data class ToolInfo(
    val name: String = "",
    val description: String = "",
    val enabled: Boolean = true,
    @SerialName("source_type") val sourceType: String = "builtin",
    val source: String? = null,
)

/** Description of one skill available in the gateway. */
@Serializable
data class SkillInfo(
    val name: String = "",
    val description: String = "",
    val enabled: Boolean = true,
    val triggers: List<String> = emptyList(),
    val permissions: List<String> = emptyList(),
)

/** Health probe response returned by `/health`. */
@Serializable
data class HealthInfo(
    val status: String = "",
    val version: String? = null,
)

/** Response from the `/webhook` synchronous execution endpoint. */
@Serializable
data class WebhookResponse(
    val response: String = "",
    val model: String = "",
    @SerialName("session_id") val sessionId: String? = null,
)
