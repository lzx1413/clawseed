package dev.clawseed.demo.updater

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Download progress for APK update.
 */
data class ApkDownloadProgress(
    val downloadedBytes: Long,
    val totalBytes: Long,
    val percent: Int,  // 0-100
) {
    val isComplete: Boolean get() = percent >= 100
}

/**
 * Downloads APK files for app updates with progress reporting.
 *
 * Files are stored in the app's cache directory under "updates/".
 * Old APK files are cleaned up before each download.
 */
class ApkDownloader(private val context: Context) {

    companion object {
        private const val UPDATE_DIR = "updates"
    }

    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .build()

    private fun updateDir(): File {
        val dir = File(context.cacheDir, UPDATE_DIR)
        if (!dir.exists()) dir.mkdirs()
        return dir
    }

    /**
     * Download an APK from the given URL, emitting progress updates.
     *
     * @param url Download URL (GitHub asset browser_download_url).
     * @param expectedSize Expected file size in bytes (from GitHub asset metadata).
     * @return Flow of [ApkDownloadProgress].
     */
    fun download(url: String, expectedSize: Long): Flow<ApkDownloadProgress> = flow {
        // Clean up old downloads before starting
        cleanup()

        val request = Request.Builder().url(url).build()
        val response = client.newCall(request).execute()

        if (!response.isSuccessful) {
            val code = response.code
            response.close()
            throw Exception("Download failed: HTTP $code")
        }

        val body = response.body ?: throw Exception("Empty response body")
        val totalBytes = if (expectedSize > 0) expectedSize else body.contentLength().coerceAtLeast(1)

        val targetFile = File(updateDir(), "update.apk")
        var downloadedBytes = 0L
        var lastPercent = -1

        body.byteStream().use { input ->
            targetFile.outputStream().use { output ->
                val buffer = ByteArray(8192)
                while (true) {
                    val read = input.read(buffer)
                    if (read == -1) break
                    output.write(buffer, 0, read)
                    downloadedBytes += read

                    val percent = ((downloadedBytes * 100) / totalBytes).toInt().coerceIn(0, 100)
                    if (percent != lastPercent) {
                        lastPercent = percent
                        emit(ApkDownloadProgress(downloadedBytes, totalBytes, percent))
                    }
                }
            }
        }

        // Final emit to ensure 100% is reached
        if (lastPercent < 100) {
            emit(ApkDownloadProgress(downloadedBytes, totalBytes, 100))
        }
    }.flowOn(Dispatchers.IO)

    /**
     * Get the downloaded APK file if it exists.
     */
    fun getDownloadedApk(): File? {
        val file = File(updateDir(), "update.apk")
        return if (file.exists() && file.length() > 0) file else null
    }

    /**
     * Remove any previously downloaded APK files.
     */
    fun cleanup() {
        updateDir().listFiles()?.forEach { it.delete() }
    }
}
