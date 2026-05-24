package dev.clawseed.sdk.core.model

/** Progress of an embedding model file download during gateway startup. */
data class EmbeddingDownloadProgress(
    /** Name of the file being downloaded (e.g. "model_int8.onnx"). */
    val filename: String,
    /** Bytes downloaded so far. */
    val downloadedBytes: Long,
    /** Total file size in bytes, or null if Content-Length was not provided. */
    val totalBytes: Long?,
    /** Download percentage (0-100), or null if total size is unknown. */
    val percent: Int?,
    /** Whether the download has completed. */
    val isComplete: Boolean = false,
)