package dev.clawseed.sdk.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Original bytes stay on the originating client; only bounded excerpts cross the gateway. */
@Serializable
data class FileAttachment(
    val id: String,
    val name: String,
    @SerialName("mime_type") val mimeType: String,
    @SerialName("size_bytes") val sizeBytes: Long,
    val format: String,
    @OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)
    @kotlinx.serialization.EncodeDefault
    val status: String = "ready",
    val unit: String,
    val total: Int,
    val excerpt: AttachmentReadResult,
)

/** Zero-based, end-exclusive offsets. Text offsets count Unicode code points. */
@Serializable
data class AttachmentReadResult(
    @SerialName("attachment_id") val attachmentId: String,
    val unit: String,
    val start: Int,
    val next: Int,
    val total: Int,
    val eof: Boolean,
    val content: String,
)

object FileAttachmentLimits {
    const val MAX_FILES = 4
    const val MAX_FILE_BYTES = 20L * 1024 * 1024
    const val MAX_IMPORT_BYTES = 50L * 1024 * 1024
    const val MAX_PARSED_CHARS = 2_000_000
    const val MAX_UNIT_CHARS = 65_536
    const val MAX_PAGES = 300
    const val AUTO_FILE_CHARS = 8_000
    const val AUTO_MESSAGE_CHARS = 16_000
    const val PREVIEW_CHARS = 1_000
    const val READ_CHARS = 16_000
}
