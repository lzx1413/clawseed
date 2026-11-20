package dev.clawseed.sdk.embedded

import android.content.Context
import android.os.Build
import android.system.Os
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.Locale
import java.util.UUID
import java.nio.channels.FileChannel
import java.nio.file.StandardOpenOption
import java.util.zip.GZIPInputStream
import java.util.zip.ZipInputStream

/** Configuration for downloading and storing a PRoot Linux runtime. */
data class ProotRuntimeConfig(
    /** Signed/controlled manifest endpoint. Empty means download is disabled. */
    val manifestUrl: String? = null,
    val connectTimeoutMs: Int = 15_000,
    val readTimeoutMs: Int = 120_000,
    val publisherKeys: Map<String, String> = emptyMap(),
    val allowUnsignedManifest: Boolean = false,
    val expectedAbi: String = Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown",
)

enum class ProotRuntimeState {
    NOT_INSTALLED,
    READY,
    INSTALLING,
    CORRUPTED,
    FAILED,
}

data class ProotRuntimeStatus(
    val state: ProotRuntimeState,
    val version: String? = null,
    val generation: String? = null,
    val path: String? = null,
    val error: String? = null,
)

data class ProotRuntimeProgress(
    val phase: String,
    val downloadedBytes: Long = 0L,
    val totalBytes: Long? = null,
)

/**
 * Installs a versioned PRoot bundle into app-private storage.
 *
 * The first bundle format is a gzip-compressed tar containing `proot` and `rootfs/`. A
 * ZIP fallback is kept for development fixtures. The
 * active pointer is written only after download, hashing, extraction, and
 * layout validation all succeed.
 */
class ProotRuntimeManager(
    context: Context,
    private val config: ProotRuntimeConfig = ProotRuntimeConfig(),
) {
    private val root = File(context.filesDir, "proot-runtime")
    private val versions = File(root, "versions")
    private val staging = File(root, "staging")
    private val activeFile = File(root, "active.json")
    private val installLockFile = File(root, "install.lock")
    private val stateFile = File(root, "state.json")
    private val manifests = File(root, "manifests")

    suspend fun ensureReady(
        onProgress: (ProotRuntimeProgress) -> Unit = {},
    ): ProotRuntimeStatus = withContext(Dispatchers.IO) {
        status().takeIf { it.state == ProotRuntimeState.READY } ?: run {
            val manifestUrl = config.manifestUrl?.trim().orEmpty()
            if (manifestUrl.isEmpty()) {
                status()
            } else {
                runCatching { install(manifestUrl, onProgress) }
                    .getOrElse { error ->
                        Log.e(TAG, "Failed to install PRoot runtime", error)
                        writeState("FAILED", error.message)
                        ProotRuntimeStatus(
                            state = ProotRuntimeState.FAILED,
                            error = error.message ?: "runtime installation failed",
                        )
                    }
            }
        }
    }

    fun status(): ProotRuntimeStatus {
        val active = activePointer() ?: return ProotRuntimeStatus(ProotRuntimeState.NOT_INSTALLED)
        val directory = File(versions, active.generation)
        return if (isValidRuntime(directory)) {
            ProotRuntimeStatus(
                ProotRuntimeState.READY,
                active.runtimeId,
                active.generation,
                directory.absolutePath,
            )
        } else {
            ProotRuntimeStatus(
                state = ProotRuntimeState.CORRUPTED,
                version = active.runtimeId,
                generation = active.generation,
                path = directory.absolutePath,
                error = "active runtime is incomplete",
            )
        }
    }

    fun activePath(): File? = status().takeIf { it.state == ProotRuntimeState.READY }
        ?.path
        ?.let(::File)

    fun clearInstalled(): Boolean = root.exists() && root.deleteRecursively()

    private fun install(
        manifestUrl: String,
        onProgress: (ProotRuntimeProgress) -> Unit,
    ): ProotRuntimeStatus = withInstallLock {
        writeState("INSTALLING")
        recoverStaging()
        installUnlocked(manifestUrl, onProgress)
    }

    private fun installUnlocked(
        manifestUrl: String,
        onProgress: (ProotRuntimeProgress) -> Unit,
    ): ProotRuntimeStatus {
        onProgress(ProotRuntimeProgress("manifest"))
        require(isAllowedUrl(manifestUrl)) { "runtime manifest must use HTTPS" }
        val manifest = requestText(manifestUrl)
        val json = JSONObject(manifest)
        require(json.optInt("schema_version", 0) == 1) { "unsupported runtime manifest schema" }
        val version = json.getString("runtime_version").trim()
        require(VERSION_PATTERN.matches(version)) { "invalid runtime version" }
        val runtimeId = json.getString("runtime_id").trim()
        require(RUNTIME_ID_PATTERN.matches(runtimeId)) { "invalid runtime id" }
        require(json.getString("abi") == config.expectedAbi) {
            "runtime ABI does not match device ABI ${config.expectedAbi}"
        }
        require(json.getInt("min_android") <= Build.VERSION.SDK_INT) {
            "runtime requires Android ${json.getInt("min_android")}, device is ${Build.VERSION.SDK_INT}"
        }
        val archiveUrl = json.getString("archive_url").trim()
        require(isAllowedUrl(archiveUrl)) { "runtime archive must use HTTPS" }
        val archiveSize = json.getLong("archive_size")
        require(archiveSize > 0L) { "archive_size must be positive" }
        val expectedSha256 = json.getString("archive_sha256").lowercase()
        require(SHA256_PATTERN.matches(expectedSha256)) { "invalid archive_sha256" }
        val archiveFormat = json.optString("archive_format", "tar.gz").lowercase(Locale.ROOT)
        require(archiveFormat == "tar.gz" || archiveFormat == "zip") {
            "unsupported runtime archive format: $archiveFormat"
        }

        val publisherKeyId = json.optString("publisher_key_id").trim()
        val signature = json.optString("signature").trim()
        if (signature.isEmpty()) {
            require(config.allowUnsignedManifest) { "runtime manifest signature is required" }
        } else {
            val publicKey = config.publisherKeys[publisherKeyId]
                ?: error("unknown runtime publisher key: $publisherKeyId")
            require(RuntimeManifestVerifier.verify(json, publicKey)) {
                "runtime manifest signature verification failed"
            }
        }

        val active = activePointer()
        if (active?.runtimeId == runtimeId) {
            val activeDirectory = File(versions, active.generation)
            val activeArchive = File(root, "archives/$runtimeId.${if (archiveFormat == "zip") "zip" else "tar.gz"}")
            if (isValidRuntime(activeDirectory) && sha256(activeArchive) == expectedSha256) {
                writeState("READY")
                return status()
            }
        }

        val generation = "$runtimeId-${UUID.randomUUID()}"
        val target = File(versions, generation)
        val archiveName = if (archiveFormat == "zip") "$runtimeId.zip" else "$runtimeId.tar.gz"
        val archive = File(root, "archives/$archiveName")

        root.mkdirs()
        versions.mkdirs()
        val generationStaging = File(staging, generation)
        generationStaging.deleteRecursively()
        generationStaging.mkdirs()
        archive.parentFile?.mkdirs()
        onProgress(ProotRuntimeProgress("download"))
        download(archiveUrl, archive, archiveSize, onProgress)
        require(archive.length() == archiveSize) { "runtime archive size mismatch" }
        require(sha256(archive) == expectedSha256) { "runtime archive SHA-256 mismatch" }

        onProgress(ProotRuntimeProgress("extract"))
        extractArchive(archive, generationStaging, archiveFormat)
        File(generationStaging, "manifest.json").writeText(manifest)
        require(isValidRuntime(generationStaging)) {
            "runtime archive must contain executable proot and rootfs/"
        }

        target.parentFile?.mkdirs()
        require(generationStaging.renameTo(target)) { "could not activate extracted runtime" }
        manifests.mkdirs()
        File(manifests, "${config.expectedAbi}-$runtimeId.json").writeText(manifest)
        writeActive(runtimeId, generation)
        writeState("READY")
        onProgress(ProotRuntimeProgress("ready"))
        return status()
    }

    private fun requestText(url: String): String {
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = config.connectTimeoutMs
            readTimeout = config.readTimeoutMs
            requestMethod = "GET"
            instanceFollowRedirects = false
            setRequestProperty("Accept", "application/json")
        }
        return try {
            val response = connection
            check(response.responseCode in 200..299) { "manifest request failed: ${response.responseCode}" }
            response.inputStream.bufferedReader(Charsets.UTF_8).use { it.readText() }
        } finally {
            connection.disconnect()
        }
    }

    private fun isAllowedUrl(url: String): Boolean =
        url.startsWith("https://") ||
            (config.allowUnsignedManifest && url.startsWith("http://"))

    private fun download(
        url: String,
        destination: File,
        expectedSize: Long,
        onProgress: (ProotRuntimeProgress) -> Unit,
    ) {
        val partial = File(destination.path + ".part")
        var offset = partial.length()
        var connection = openDownload(url, offset)
        if (offset > 0L && connection.responseCode != HttpURLConnection.HTTP_PARTIAL) {
            connection.disconnect()
            partial.delete()
            offset = 0L
            connection = openDownload(url, 0L)
        }
        try {
            val response = connection
            check(response.responseCode in 200..299) { "runtime download failed: ${response.responseCode}" }
            FileOutputStream(partial, offset > 0L).use { output ->
                response.inputStream.use { input ->
                    val contentLength = response.contentLengthLong.takeIf { it >= 0L }
                    val totalBytes = contentLength?.plus(offset)
                    if (totalBytes != null) check(totalBytes == expectedSize) {
                        "runtime response size mismatch"
                    }
                    var downloadedBytes = offset
                    val buffer = ByteArray(64 * 1024)
                    onProgress(ProotRuntimeProgress("download", downloadedBytes, totalBytes))
                    while (true) {
                        val count = input.read(buffer)
                        if (count <= 0) break
                        output.write(buffer, 0, count)
                        downloadedBytes += count
                        onProgress(ProotRuntimeProgress("download", downloadedBytes, totalBytes))
                    }
                }
            }
        } finally {
            connection.disconnect()
        }
        destination.delete()
        check(partial.renameTo(destination)) { "could not finalize runtime archive" }
    }

    private fun openDownload(url: String, offset: Long): HttpURLConnection =
        (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = config.connectTimeoutMs
            readTimeout = config.readTimeoutMs
            requestMethod = "GET"
            instanceFollowRedirects = false
            if (offset > 0L) setRequestProperty("Range", "bytes=$offset-")
        }

    private fun extractArchive(archive: File, destination: File, format: String) {
        if (format == "tar.gz") {
            extractTarGz(archive, destination)
        } else {
            extractZip(archive, destination)
        }
    }

    private fun extractZip(archive: File, destination: File) {
        val canonicalDestination = destination.canonicalFile
        destination.mkdirs()
        ZipInputStream(FileInputStream(archive).buffered()).use { input ->
            while (true) {
                val entry = input.nextEntry ?: break
                val output = File(destination, entry.name).canonicalFile
                require(output.path == canonicalDestination.path ||
                    output.path.startsWith(canonicalDestination.path + File.separator)) {
                    "runtime archive contains an unsafe path"
                }
                if (entry.isDirectory) {
                    output.mkdirs()
                } else {
                    output.parentFile?.mkdirs()
                    FileOutputStream(output).use { input.copyTo(it) }
                }
                input.closeEntry()
            }
        }
        File(destination, "proot").setExecutable(true, false)
    }

    private fun extractTarGz(archive: File, destination: File) {
        destination.mkdirs()
        GZIPInputStream(BufferedInputStream(FileInputStream(archive))).use { input ->
            val header = ByteArray(TAR_BLOCK_SIZE)
            while (readBlock(input, header)) {
                if (header.all { it == 0.toByte() }) break

                val name = tarString(header, 0, 100)
                val prefix = tarString(header, 345, 155)
                val entryName = if (prefix.isEmpty()) name else "$prefix/$name"
                val size = tarOctal(header, 124, 12)
                val mode = tarOctal(header, 100, 8)
                val linkName = tarString(header, 157, 100)
                val output = safeOutput(destination, entryName)
                when (header[156].toInt().toChar()) {
                    '\u0000', '0' -> {
                        output.parentFile?.mkdirs()
                        FileOutputStream(output).use { copyExactly(input, it, size) }
                        applyMode(output, mode)
                    }
                    '1' -> {
                        output.parentFile?.mkdirs()
                        val target = safeOutput(destination, linkName)
                        Os.link(target.absolutePath, output.absolutePath)
                    }
                    '2' -> {
                        output.parentFile?.mkdirs()
                        Os.symlink(linkName, output.absolutePath)
                    }
                    '5' -> output.mkdirs()
                    // GNU/PAX metadata is not part of the extracted runtime tree.
                    'L', 'K', 'x', 'g' -> skipExactly(input, size)
                    else -> skipExactly(input, size)
                }
                skipPadding(input, size)
            }
        }
        File(destination, "proot").setExecutable(true, false)
    }

    private fun safeOutput(destination: File, entryName: String): File {
        require(entryName.isNotEmpty()) { "runtime archive contains an empty path" }
        val canonicalDestination = destination.canonicalFile
        val output = File(destination, entryName).canonicalFile
        require(output.path.startsWith(canonicalDestination.path + File.separator)) {
            "runtime archive contains an unsafe path"
        }
        return output
    }

    private fun applyMode(file: File, mode: Long) {
        if (mode and 0b001_001_001L != 0L) file.setExecutable(true, false)
        if (mode and 0b010_010_010L != 0L) file.setWritable(true, false)
    }

    private fun tarString(bytes: ByteArray, offset: Int, length: Int): String {
        val end = (offset until offset + length).firstOrNull { bytes[it].toInt() == 0 || bytes[it].toInt() == 32 }
            ?: (offset + length)
        return bytes.copyOfRange(offset, end).toString(Charsets.UTF_8).trim()
    }

    private fun tarOctal(bytes: ByteArray, offset: Int, length: Int): Long {
        val value = tarString(bytes, offset, length)
        return if (value.isEmpty()) 0L else value.toLong(8)
    }

    private fun readBlock(input: InputStream, block: ByteArray): Boolean {
        var offset = 0
        while (offset < block.size) {
            val count = input.read(block, offset, block.size - offset)
            if (count < 0) return offset != 0 && error("truncated tar header")
            offset += count
        }
        return true
    }

    private fun copyExactly(input: InputStream, output: FileOutputStream, size: Long) {
        var remaining = size
        val buffer = ByteArray(64 * 1024)
        while (remaining > 0L) {
            val count = input.read(buffer, 0, minOf(buffer.size.toLong(), remaining).toInt())
            check(count > 0) { "truncated runtime archive entry" }
            output.write(buffer, 0, count)
            remaining -= count
        }
    }

    private fun skipExactly(input: InputStream, size: Long) {
        var remaining = size
        while (remaining > 0L) {
            val skipped = input.skip(remaining)
            if (skipped > 0L) {
                remaining -= skipped
            } else {
                check(input.read() >= 0) { "truncated runtime archive entry" }
                remaining -= 1L
            }
        }
    }

    private fun skipPadding(input: InputStream, size: Long) {
        val padding = (TAR_BLOCK_SIZE - (size % TAR_BLOCK_SIZE)) % TAR_BLOCK_SIZE
        skipExactly(input, padding)
    }

    private fun isValidRuntime(directory: File): Boolean =
        File(directory, "proot").isFile &&
            File(directory, "proot").canExecute() &&
            File(directory, "rootfs").isDirectory &&
            File(directory, "manifest.json").isFile

    private data class ActivePointer(val runtimeId: String, val generation: String)

    private fun activePointer(): ActivePointer? {
        if (!activeFile.isFile) return null
        return runCatching {
            val json = JSONObject(activeFile.readText())
            ActivePointer(json.getString("runtime_id"), json.getString("generation"))
        }.getOrNull()
    }

    private fun writeActive(runtimeId: String, generation: String) {
        root.mkdirs()
        val temporary = File(root, "active.json.tmp")
        temporary.writeText(
            JSONObject()
                .put("runtime_id", runtimeId)
                .put("generation", generation)
                .toString(),
        )
        check(temporary.renameTo(activeFile)) { "could not update active runtime" }
    }

    private fun <T> withInstallLock(block: () -> T): T {
        root.mkdirs()
        FileChannel.open(
            installLockFile.toPath(),
            StandardOpenOption.CREATE,
            StandardOpenOption.WRITE,
        ).use { channel ->
            val lock = channel.lock()
            return try {
                block()
            } finally {
                lock.release()
            }
        }
    }

    private fun recoverStaging() {
        staging.listFiles()?.forEach { entry -> entry.deleteRecursively() }
    }

    private fun writeState(state: String, error: String? = null) {
        root.mkdirs()
        val json = JSONObject()
            .put("state", state)
            .put("updated_at_ms", System.currentTimeMillis())
        if (!error.isNullOrBlank()) json.put("error", error.take(500))
        val temporary = File(root, "state.json.tmp")
        temporary.writeText(json.toString())
        temporary.renameTo(stateFile)
    }

    private fun sha256(file: File): String {
        if (!file.isFile) return ""
        val digest = MessageDigest.getInstance("SHA-256")
        FileInputStream(file).use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
                val count = input.read(buffer)
                if (count <= 0) break
                digest.update(buffer, 0, count)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    companion object {
        private const val TAG = "ProotRuntimeManager"
        private const val TAR_BLOCK_SIZE = 512
        private val VERSION_PATTERN = Regex("[A-Za-z0-9._-]{1,80}")
        private val RUNTIME_ID_PATTERN = Regex("[A-Za-z0-9._+-]{1,120}")
        private val SHA256_PATTERN = Regex("[0-9a-f]{64}")
    }
}
