package dev.clawseed.demo.updater

import android.content.Context
import android.os.Build
import dev.clawseed.demo.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import java.util.concurrent.TimeUnit

/**
 * Information about an available app update from GitHub Release.
 */
data class UpdateInfo(
    val versionName: String,
    val versionCode: Int,
    val downloadUrl: String,
    val downloadSize: Long,
    val releaseNotes: String,
    val htmlUrl: String,
)

@Serializable
private data class GithubRelease(
    val tag_name: String = "",
    val body: String = "",
    val html_url: String = "",
    val assets: List<GithubAsset> = emptyList(),
)

@Serializable
private data class GithubAsset(
    val name: String = "",
    val size: Long = 0,
    val browser_download_url: String = "",
)

/**
 * Checks GitHub Releases for a newer version of the app.
 *
 * APK naming convention: `clawseed-{version}-{abi}.apk`
 * where {abi} is arm64-v8a, x86_64, or armeabi-v7a.
 */
class AppUpdateChecker(private val context: Context) {

    companion object {
        private const val RELEASE_API_URL =
            "https://api.github.com/repos/lzx1413/clawseed/releases/latest"
        private const val APK_PREFIX = "clawseed-"
        private const val APK_SUFFIX = ".apk"

        /** Map Android Build.SUPPORTED_ABIS to our APK ABI suffix. */
        fun preferredAbi(): String {
            val primaryAbi = Build.SUPPORTED_ABIS.firstOrNull() ?: "arm64-v8a"
            return when {
                primaryAbi.startsWith("arm64") -> "arm64-v8a"
                primaryAbi.startsWith("x86_64") -> "x86_64"
                primaryAbi.startsWith("armeabi") -> "armeabi-v7a"
                else -> "arm64-v8a"
            }
        }

        /**
         * Parse version code from a tag like "v1.9" or "1.9".
         * Returns null if the tag cannot be parsed.
         */
        fun parseVersionCode(tag: String): Int? {
            val digits = tag.removePrefix("v").removePrefix("V")
            val parts = digits.split(".")
            if (parts.isEmpty()) return null
            // major * 100 + minor (e.g. 1.9 -> 109, 2.0 -> 200)
            val major = parts.getOrNull(0)?.toIntOrNull() ?: return null
            val minor = parts.getOrNull(1)?.toIntOrNull() ?: 0
            val patch = parts.getOrNull(2)?.toIntOrNull() ?: 0
            return major * 10000 + minor * 100 + patch
        }

        /**
         * Parse version name from a tag like "v1.9" or "1.9.3".
         */
        fun parseVersionName(tag: String): String {
            return tag.removePrefix("v").removePrefix("V")
        }
    }

    private val json = Json { ignoreUnknownKeys = true }

    private val client = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .build()

    /**
     * Check GitHub for a newer release.
     *
     * @return [UpdateInfo] if a newer version is available, null if already up-to-date.
     * @throws Exception on network errors or API failures.
     */
    suspend fun checkUpdate(): UpdateInfo? = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url(RELEASE_API_URL)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "ClawSeed/${BuildConfig.VERSION_NAME}")
            .build()

        val response = client.newCall(request).execute()
        if (!response.isSuccessful) {
            val code = response.code
            response.close()
            throw Exception("GitHub API returned HTTP $code")
        }

        val body = response.body?.string() ?: throw Exception("Empty response body")
        val release = json.decodeFromString<GithubRelease>(body)

        val remoteVersionCode = parseVersionCode(release.tag_name)
            ?: throw Exception("Cannot parse version from tag: ${release.tag_name}")

        // Compare using version-name-derived codes, not BuildConfig.VERSION_CODE
        // (VERSION_CODE is a sequential integer unrelated to the version name)
        val currentVersionCode = parseVersionCode(BuildConfig.VERSION_NAME)
            ?: throw Exception("Cannot parse current version: ${BuildConfig.VERSION_NAME}")

        // No update if remote version is not newer
        if (remoteVersionCode <= currentVersionCode) {
            return@withContext null
        }

        // Find matching APK for device ABI
        val abi = preferredAbi()
        val expectedName = "$APK_PREFIX${parseVersionName(release.tag_name)}-$abi$APK_SUFFIX"
        // Try: exact name → ABI suffix → any single APK
        val asset = release.assets.find { it.name == expectedName }
            ?: release.assets.find { it.name.endsWith("-$abi$APK_SUFFIX") }
            ?: release.assets.find { it.name.endsWith(APK_SUFFIX) }
            ?: throw Exception("No APK found for ABI $abi in release ${release.tag_name}")

        UpdateInfo(
            versionName = parseVersionName(release.tag_name),
            versionCode = remoteVersionCode,
            downloadUrl = asset.browser_download_url,
            downloadSize = asset.size,
            releaseNotes = release.body,
            htmlUrl = release.html_url,
        )
    }
}
