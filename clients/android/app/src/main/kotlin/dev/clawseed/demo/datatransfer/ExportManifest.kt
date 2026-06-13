package dev.clawseed.demo.datatransfer

import kotlinx.serialization.Serializable

/** Metadata stored inside the export ZIP as manifest.json. */
@Serializable
data class ExportManifest(
    /** Format version — starts at 1. Import rejects if version > current supported. */
    val version: Int = CURRENT_VERSION,
    /** Epoch millis when the export was created. */
    val timestamp: Long,
    /** App version that created the export. */
    val appVersion: String,
    /** Which categories are included in this ZIP. */
    val categories: List<String>,
    /** Whether sensitive data (API keys, tokens) was excluded. */
    val excludeSensitive: Boolean = false,
) {
    companion object {
        const val CURRENT_VERSION = 1
    }
}
