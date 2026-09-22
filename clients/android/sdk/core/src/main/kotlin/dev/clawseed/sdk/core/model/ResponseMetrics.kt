package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Provider-reported counts for one assistant turn.
 * Input and cached-input counts refer to the latest provider request, while
 * output counts may include the complete tool loop. Null means unavailable,
 * not zero. Speed excludes tools and first-token wait.
 */
@Serializable
data class ResponseMetrics(
    @SerialName("input_tokens") val inputTokens: Long? = null,
    @SerialName("output_tokens") val outputTokens: Long? = null,
    @SerialName("cached_input_tokens") val cachedInputTokens: Long? = null,
    @SerialName("cache_hit_ratio") val cacheHitRatio: Double? = null,
    @SerialName("output_tokens_per_second") val outputTokensPerSecond: Double? = null,
    @SerialName("elapsed_ms") val elapsedMs: Long? = null,
)
