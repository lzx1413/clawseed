package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Durable metadata; image bytes are fetched through the authenticated session API. */
@Serializable
data class ImageAttachment(
    val id: String,
    @SerialName("mime_type") val mimeType: String,
    @SerialName("size_bytes") val sizeBytes: Long,
    val width: Int,
    val height: Int,
)

@Serializable
data class ImageAttachmentCapability(
    val supported: Boolean = false,
    @SerialName("model_support") val modelSupport: String = "unknown",
    @SerialName("max_images_per_message") val maxImagesPerMessage: Int = 4,
    @SerialName("max_image_bytes") val maxImageBytes: Long = 5 * 1024 * 1024,
    @SerialName("max_dimension") val maxDimension: Int = 8192,
)
