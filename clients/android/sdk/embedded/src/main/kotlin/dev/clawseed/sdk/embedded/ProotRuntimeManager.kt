package dev.clawseed.sdk.embedded

import android.content.Context
import android.os.Build
import android.os.PowerManager
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.Locale
import java.util.UUID
import java.nio.channels.FileChannel
import java.nio.file.StandardOpenOption
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock
import kotlinx.coroutines.CancellationException

/** Configuration for downloading and storing the shared Linux user-space runtime. */
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
    val canRollback: Boolean = false,
    val checkedAtMs: Long? = null,
    val backend: String? = null,
)

data class ProotRuntimeProgress(
    val phase: String,
    val downloadedBytes: Long = 0L,
    val totalBytes: Long? = null,
)

/**
 * Installs a versioned Linux runtime bundle into app-private storage.
 *
 * Bundled assets are authenticated by APK signing; remote manifests need a trusted
 * publisher signature. tar.gz/tar.xz and development ZIP bundles are supported.
 * Activation follows hash verification, safe extraction and guest execution checks.
 */
class ProotRuntimeManager(
    private val context: Context,
    private val config: ProotRuntimeConfig = ProotRuntimeConfig(),
) {
    private val root = File(context.filesDir, "proot-runtime")
    private val versions = File(root, "versions")
    private val staging = File(root, "staging")
    private val activeFile = File(root, "active.json")
    private val installLockFile = File(root, "install.lock")
    private val stateFile = File(root, "state.json")
    private val manifests = File(root, "manifests")

    fun hasInstallSource(): Boolean = config.manifestUrl?.isNotBlank() == true ||
        runCatching { context.assets.open(BUNDLED_MANIFEST).use { } }.isSuccess

    suspend fun ensureReady(
        onProgress: (ProotRuntimeProgress) -> Unit = {},
    ): ProotRuntimeStatus = install(false, onProgress)

    /** Prepare and check a new generation while retaining the current one for rollback. */
    suspend fun updateInstalled(
        onProgress: (ProotRuntimeProgress) -> Unit = {},
    ): ProotRuntimeStatus = install(true, onProgress)

    private suspend fun install(force: Boolean, onProgress: (ProotRuntimeProgress) -> Unit): ProotRuntimeStatus = withContext(Dispatchers.IO) {
        if (!force && status().state == ProotRuntimeState.READY) return@withContext status()
        try {
            withInstallLock {
                if (!force && status().state == ProotRuntimeState.READY) return@withInstallLock status()
                if (!hasInstallSource()) return@withInstallLock status()
                withIdleRuntime {
                val wakeLock = context.getSystemService(PowerManager::class.java)
                    .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "ClawSeed:runtime-install")
                wakeLock.acquire(10 * 60 * 1000L)
                try {
                    writeState("INSTALLING")
                    recoverStaging()
                    val bundled = runCatching { context.assets.open(BUNDLED_MANIFEST)
                        .bufferedReader().use { it.readText() } }.getOrNull()
                    onProgress(ProotRuntimeProgress("manifest"))
                    if (bundled != null) {
                        installUnlocked(bundled, true, onProgress)
                    } else {
                        val url = config.manifestUrl!!.trim()
                        require(isAllowedUrl(url)) { "runtime manifest must use HTTPS" }
                        installUnlocked(requestText(url), false, onProgress)
                    }
                } finally {
                    if (wakeLock.isHeld) wakeLock.release()
                }
                }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Log.e(TAG, "Failed to install Linux runtime", error)
            writeState("FAILED", error.message)
            val previous = status()
            if (previous.state == ProotRuntimeState.READY) previous.copy(error = error.message)
            else ProotRuntimeStatus(ProotRuntimeState.FAILED, error = error.message ?: "runtime installation failed")
        }
    }

    suspend fun checkHealth(): ProotRuntimeStatus = withContext(Dispatchers.IO) {
        try {
            withInstallLock { withIdleRuntime {
                val active = activePointer() ?: error("Linux runtime is not installed")
                val directory = File(versions, active.generation)
                runHealthCheck(directory)
                status()
            } }
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) { status().copy(error = error.message) }
    }

    suspend fun rollback(): ProotRuntimeStatus = withContext(Dispatchers.IO) {
        try {
            withInstallLock { withIdleRuntime {
                val active = activePointer() ?: error("Linux runtime is not installed")
                val previous = active.previousGeneration ?: error("No previous Linux runtime is available")
                val directory = File(versions, previous)
                require(isValidRuntime(directory)) { "Previous Linux runtime is incomplete" }
                runHealthCheck(directory)
                val runtimeId = JSONObject(File(directory, "manifest.json").readText()).getString("runtime_id")
                writeActive(runtimeId, previous, active.generation)
                writeState("READY")
                status()
            } }
        } catch (error: CancellationException) { throw error }
        catch (error: Exception) { status().copy(error = error.message) }
    }

    private fun runHealthCheck(directory: File) {
        val wakeLock = context.getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "ClawSeed:runtime-check")
        wakeLock.acquire(2 * 60 * 1000L)
        val health = JSONObject().put("checked_at_ms", System.currentTimeMillis())
        try {
            RuntimeReadinessProbe.check(context, directory)
            health.put("ok", true)
        } catch (error: Exception) {
            health.put("ok", false).put("error", error.message?.take(2000))
            throw error
        } finally {
            try {
                val temporary = File(directory, "health.json.tmp")
                temporary.writeText(health.toString())
                check(temporary.renameTo(File(directory, "health.json"))) { "Could not save runtime health check" }
            } finally { if (wakeLock.isHeld) wakeLock.release() }
        }
    }

    fun status(): ProotRuntimeStatus {
        val active = activePointer() ?: return runCatching {
            val state = JSONObject(stateFile.readText())
            if (state.optString("state") == "FAILED") {
                ProotRuntimeStatus(ProotRuntimeState.FAILED, error = state.optString("error"))
            } else ProotRuntimeStatus(ProotRuntimeState.NOT_INSTALLED)
        }.getOrDefault(ProotRuntimeStatus(ProotRuntimeState.NOT_INSTALLED))
        val directory = File(versions, active.generation)
        val healthFile = File(directory, "health.json")
        val health = runCatching { JSONObject(healthFile.readText()) }.getOrNull()
        val checkedAt = health?.optLong("checked_at_ms")?.takeIf { it > 0 }
        val canRollback = active.previousGeneration?.let { isValidRuntime(File(versions, it)) } == true
        return if (isValidRuntime(directory) && (!healthFile.exists() || health?.optBoolean("ok", false) == true)) {
            ProotRuntimeStatus(
                ProotRuntimeState.READY,
                active.runtimeId,
                active.generation,
                directory.absolutePath,
                canRollback = canRollback,
                checkedAtMs = checkedAt,
                backend = preferredBackend(),
            )
        } else {
            ProotRuntimeStatus(
                state = ProotRuntimeState.CORRUPTED,
                version = active.runtimeId,
                generation = active.generation,
                path = directory.absolutePath,
                error = health?.optString("error")?.takeIf { it.isNotBlank() } ?: "active runtime is incomplete",
                canRollback = canRollback,
                checkedAtMs = checkedAt,
                backend = preferredBackend(),
            )
        }
    }

    private fun preferredBackend(): String =
        if (File(context.applicationInfo.nativeLibraryDir, "libtawcroot.so").canExecute()) "tawcroot" else "proot"

    fun activePath(): File? = status().takeIf { it.state == ProotRuntimeState.READY }
        ?.path
        ?.let(::File)

    fun clearInstalled(): Boolean = withInstallLock {
        // Keep both lock inodes stable while the gateway is alive. linux_exec
        // holds a shared POSIX lock for the full lifetime of each task.
        withIdleRuntime {
                listOf(activeFile, stateFile, versions, staging, manifests, File(root, "archives"))
                    .all { RuntimeFiles.deleteTree(it) }
        }
    }

    private fun <T> withIdleRuntime(block: () -> T): T {
        FileChannel.open(File(root, "usage.lock").toPath(), StandardOpenOption.CREATE,
            StandardOpenOption.READ, StandardOpenOption.WRITE).use { channel ->
            val lease = channel.tryLock() ?: error("Linux commands are running; wait for them to finish before changing or checking the runtime")
            try { return block() } finally { lease.release() }
        }
    }

    private fun installUnlocked(
        manifest: String,
        bundled: Boolean,
        onProgress: (ProotRuntimeProgress) -> Unit,
    ): ProotRuntimeStatus {
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
        val archiveUrl = json.optString("archive_url").trim()
        if (!bundled) require(isAllowedUrl(archiveUrl)) { "runtime archive must use HTTPS" }
        val archiveSize = json.getLong("archive_size")
        require(archiveSize > 0L) { "archive_size must be positive" }
        val expectedSha256 = json.getString("archive_sha256").lowercase()
        require(SHA256_PATTERN.matches(expectedSha256)) { "invalid archive_sha256" }
        val archiveFormat = json.optString("archive_format", "tar.gz").lowercase(Locale.ROOT)
        require(archiveFormat in setOf("tar.gz", "tar.xz", "zip")) {
            "unsupported runtime archive format: $archiveFormat"
        }

        val publisherKeyId = json.optString("publisher_key_id").trim()
        val signature = json.optString("signature").trim()
        // APK signing authenticates bundled manifests and expected archive digests.
        if (!bundled && signature.isEmpty()) {
            require(config.allowUnsignedManifest) { "runtime manifest signature is required" }
        } else if (!bundled) {
            val publicKey = config.publisherKeys[publisherKeyId]
                ?: error("unknown runtime publisher key: $publisherKeyId")
            require(RuntimeManifestVerifier.verify(json, publicKey)) {
                "runtime manifest signature verification failed"
            }
        }

        val active = activePointer()

        val generation = "$runtimeId-${UUID.randomUUID()}"
        val target = File(versions, generation)
        val archiveName = "$expectedSha256.$archiveFormat"
        val archive = File(root, "archives/$archiveName")

        root.mkdirs()
        versions.mkdirs()
        val generationStaging = File(staging, generation)
        check(RuntimeFiles.deleteTree(generationStaging)) { "could not clean runtime staging" }
        generationStaging.mkdirs()
        archive.parentFile?.mkdirs()
        val unpackedSize = json.optLong("unpacked_size", 512L * 1024 * 1024)
        require(unpackedSize in 1..(2L * 1024 * 1024 * 1024)) { "invalid unpacked runtime size" }
        val reserve = unpackedSize + unpackedSize / 5 + if (bundled) 0L else archiveSize
        require(context.filesDir.usableSpace >= reserve) { "not enough storage for Linux runtime" }
        val input = if (bundled) {
            // Read directly from the APK: do not retain a second compressed copy.
            onProgress(ProotRuntimeProgress("verify"))
            val asset = "$BUNDLED_DIR/runtime.$archiveFormat"
            val digest = MessageDigest.getInstance("SHA-256")
            var bytes = 0L
            context.assets.open(asset).use { stream ->
                val buffer = ByteArray(64 * 1024)
                while (true) {
                    val count = stream.read(buffer)
                    if (count < 0) break
                    bytes += count
                    require(bytes <= archiveSize) { "bundled runtime size mismatch" }
                    digest.update(buffer, 0, count)
                    onProgress(ProotRuntimeProgress("verify", bytes, archiveSize))
                }
            }
            require(bytes == archiveSize && digest.digest().joinToString("") { "%02x".format(it) } == expectedSha256) {
                "bundled runtime SHA-256/size mismatch"
            }
            context.assets.open(asset)
        } else {
            onProgress(ProotRuntimeProgress("download"))
            download(archiveUrl, archive, archiveSize, onProgress)
            if (archive.length() != archiveSize || sha256(archive) != expectedSha256) {
                archive.delete()
                error("runtime archive SHA-256/size mismatch")
            }
            FileInputStream(archive)
        }
        onProgress(ProotRuntimeProgress("extract"))
        input.use { RuntimeArchiveExtractor.extract(it, generationStaging, archiveFormat, unpackedSize) }
        File(generationStaging, "manifest.json").writeText(manifest)
        require(isValidRuntime(generationStaging)) {
            "runtime archive must contain executable proot and rootfs/"
        }

        if (json.has("components")) {
            for (path in listOf("usr/bin/env", "bin/sh", "opt/node/bin/node", "opt/python/bin/python3")) {
                require(File(generationStaging, "rootfs/$path").canExecute()) { "runtime component is missing: $path" }
            }
            onProgress(ProotRuntimeProgress("check"))
            runHealthCheck(generationStaging)
        }
        target.parentFile?.mkdirs()
        require(generationStaging.renameTo(target)) { "could not activate extracted runtime" }
        manifests.mkdirs()
        File(manifests, "${config.expectedAbi}-$runtimeId.json").writeText(manifest)
        writeActive(runtimeId, generation, active?.generation)
        writeState("READY")
        // Never remove the preceding environment or its manually installed dependencies.
        // All users are excluded by usage.lock while older generations are pruned.
        versions.listFiles()?.filter { it.name != generation && it.name != active?.generation }
            ?.forEach { RuntimeFiles.deleteTreeOrThrow(it) }
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
                        downloadedBytes += count
                        check(downloadedBytes <= expectedSize) { "runtime download exceeds declared size" }
                        output.write(buffer, 0, count)
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

    private fun isValidRuntime(directory: File): Boolean {
        if (!((File(directory, "proot").canExecute() ||
            File(context.applicationInfo.nativeLibraryDir, "libproot.so").canExecute()) &&
            File(directory, "rootfs").isDirectory &&
            File(directory, "manifest.json").isFile)) return false
        return runCatching {
            val manifest = JSONObject(File(directory, "manifest.json").readText())
            !manifest.has("components") || listOf("usr/bin/env", "bin/sh", "opt/node/bin/node", "opt/python/bin/python3")
                .all { File(directory, "rootfs/$it").canExecute() }
        }.getOrDefault(false)
    }

    private data class ActivePointer(val runtimeId: String, val generation: String, val previousGeneration: String? = null)

    private fun activePointer(): ActivePointer? {
        if (!activeFile.isFile) return null
        return runCatching {
            val json = JSONObject(activeFile.readText())
            val generation = json.getString("generation")
            require(RUNTIME_ID_PATTERN.matches(generation) && generation != "." && generation != "..")
            val previous = json.optString("previous_generation").takeIf { it.isNotBlank() }
            require(previous == null || (RUNTIME_ID_PATTERN.matches(previous) && previous != "." && previous != ".." && previous != generation))
            ActivePointer(json.getString("runtime_id"), generation, previous)
        }.getOrNull()
    }

    private fun writeActive(runtimeId: String, generation: String, previousGeneration: String? = null) {
        root.mkdirs()
        val temporary = File(root, "active.json.tmp")
        temporary.writeText(
            JSONObject()
                .put("runtime_id", runtimeId)
                .put("generation", generation)
                .put("previous_generation", previousGeneration)
                .toString(),
        )
        check(temporary.renameTo(activeFile)) { "could not update active runtime" }
    }

    private fun <T> withInstallLock(block: () -> T): T = INSTALL_MUTEX.withLock {
        root.mkdirs()
        FileChannel.open(
            installLockFile.toPath(),
            StandardOpenOption.CREATE,
            StandardOpenOption.WRITE,
        ).use { channel ->
            val lock = channel.lock()
            try {
                block()
            } finally {
                lock.release()
            }
        }
    }

    private fun recoverStaging() {
        staging.listFiles()?.forEach { entry -> RuntimeFiles.deleteTreeOrThrow(entry) }
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
        private val INSTALL_MUTEX = ReentrantLock()
        private const val BUNDLED_DIR = "clawseed-runtime"
        private const val BUNDLED_MANIFEST = "$BUNDLED_DIR/manifest.json"
        private val VERSION_PATTERN = Regex("[A-Za-z0-9._-]{1,80}")
        private val RUNTIME_ID_PATTERN = Regex("[A-Za-z0-9._+-]{1,120}")
        private val SHA256_PATTERN = Regex("[0-9a-f]{64}")
    }
}
