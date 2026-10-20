package dev.clawseed.demo.datatransfer

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Credentials
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.asRequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.EOFException
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.io.StringReader
import java.net.SocketException
import java.net.SocketTimeoutException
import java.util.UUID
import java.net.URI
import java.net.URLDecoder
import java.net.URLEncoder
import java.security.SecureRandom
import java.security.spec.KeySpec
import java.util.concurrent.TimeUnit
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.CipherInputStream
import javax.crypto.CipherOutputStream
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.SecretKeySpec
import android.util.Xml
import org.xmlpull.v1.XmlPullParser

internal const val WEB_DAV_MAX_ARCHIVE_BYTES = 256L * 1024L * 1024L

private open class WebDavTransferException(
    message: String,
    cause: Throwable? = null,
    val httpCode: Int? = null,
) : IOException(message, cause)

private class RetryableWebDavException(message: String, httpCode: Int) :
    WebDavTransferException(message, httpCode = httpCode)

data class WebDavTimeouts(
    val connectSeconds: Long = 15,
    val readSeconds: Long = 180,
    val writeSeconds: Long = 180,
) {
    init {
        require(connectSeconds > 0 && readSeconds > 0 && writeSeconds > 0) {
            "WebDAV timeouts must be positive"
        }
    }
}

enum class WebDavCheckStage {
    URL,
    AUTHENTICATION,
    DIRECTORY,
    WRITE,
}

data class WebDavCheck(
    val stage: WebDavCheckStage,
    val passed: Boolean,
    val detail: String,
)

data class WebDavConnectionTestResult(val checks: List<WebDavCheck>) {
    val passed: Boolean get() = checks.all(WebDavCheck::passed)
}

/** Uploads and downloads an encrypted data-transfer archive through WebDAV. */
class WebDavSyncClient(private val client: OkHttpClient = defaultClient()) {
    constructor(timeouts: WebDavTimeouts) : this(defaultClient(timeouts))

    data class UploadResult(val bytes: Int, val checksum: String)
    data class DownloadResult(val bytes: ByteArray, val checksum: String)
    data class DownloadFileResult(val bytes: Long, val checksum: String)
    data class RemoteBackup(
        val name: String,
        val path: String,
        val bytes: Long,
        val lastModified: String?,
    )

    suspend fun upload(
        url: String,
        remotePath: String,
        archive: ByteArray,
        username: String?,
        password: String?,
        encryptionPassword: String,
    ): kotlin.Result<UploadResult> = withContext(Dispatchers.IO) {
        val archiveFile = File.createTempFile("clawseed-webdav-", ".zip")
        try {
            archiveFile.writeBytes(archive)
            uploadFile(url, remotePath, archiveFile, username, password, encryptionPassword)
        } finally {
            archiveFile.delete()
        }
    }

    /** Encrypts and uploads an archive from disk without buffering the archive in memory. */
    suspend fun uploadFile(
        url: String,
        remotePath: String,
        archiveFile: File,
        username: String?,
        password: String?,
        encryptionPassword: String,
    ): kotlin.Result<UploadResult> = withContext(Dispatchers.IO) {
        runCatching {
            validate(url, remotePath, encryptionPassword)
            require(archiveFile.isFile) { "Backup archive is missing" }
            require(archiveFile.length() <= WEB_DAV_MAX_ARCHIVE_BYTES) {
                "WebDAV backup exceeds 256 MiB"
            }
            val encryptedFile = File.createTempFile("clawseed-webdav-", ".enc", archiveFile.parentFile)
            try {
                encryptFile(archiveFile, encryptedFile, encryptionPassword)
                require(encryptedFile.length() <= WEB_DAV_MAX_ARCHIVE_BYTES) {
                    "WebDAV backup exceeds 256 MiB"
                }
                ensureCollections(url, remotePath, username, password)
                val temporaryPath = "$remotePath.tmp-${System.currentTimeMillis()}"
                val upload = request(url, temporaryPath, username, password)
                    .put(encryptedFile.asRequestBody(BINARY_MEDIA_TYPE))
                    .build()
                withRetry {
                    client.newCall(upload).executeChecked("upload WebDAV backup").close()
                }
                val move = Request.Builder()
                    .url(resolve(url, temporaryPath))
                    .header("Destination", resolve(url, remotePath))
                    .header("Overwrite", "T")
                    .method("MOVE", null)
                    .withAuth(username, password)
                    .build()
                client.newCall(move).execute().use { response ->
                    checkResponse(response, "commit WebDAV backup")
                }
                UploadResult(encryptedFile.length().toInt(), sha256(encryptedFile))
            } finally {
                encryptedFile.delete()
            }
        }
    }

    suspend fun download(
        url: String,
        remotePath: String,
        username: String?,
        password: String?,
        encryptionPassword: String,
    ): kotlin.Result<DownloadResult> = withContext(Dispatchers.IO) {
        val archiveFile = File.createTempFile("clawseed-webdav-", ".zip")
        try {
            val result = downloadFile(url, remotePath, archiveFile, username, password, encryptionPassword)
                .getOrThrow()
            Result.success(DownloadResult(archiveFile.readBytes(), result.checksum))
        } catch (error: Exception) {
            Result.failure(error)
        } finally {
            archiveFile.delete()
        }
    }

    /** Downloads and decrypts a backup to disk without buffering the payload in memory. */
    suspend fun downloadFile(
        url: String,
        remotePath: String,
        archiveFile: File,
        username: String?,
        password: String?,
        encryptionPassword: String,
    ): kotlin.Result<DownloadFileResult> = withContext(Dispatchers.IO) {
        runCatching {
            validate(url, remotePath, encryptionPassword)
            val encryptedFile = File.createTempFile("clawseed-webdav-", ".enc", archiveFile.parentFile)
            try {
                val request = request(url, remotePath, username, password).build()
                withRetry {
                    client.newCall(request).execute().use { response ->
                        checkResponse(response, "download WebDAV backup")
                        val body = response.body ?: error("WebDAV backup is empty")
                        val contentLength = body.contentLength()
                        check(contentLength <= WEB_DAV_MAX_ARCHIVE_BYTES || contentLength < 0) {
                            "WebDAV backup exceeds 256 MiB"
                        }
                        body.byteStream().use { input ->
                            BufferedOutputStream(FileOutputStream(encryptedFile)).use { output ->
                                val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                                var total = 0L
                                while (true) {
                                    val count = input.read(buffer)
                                    if (count < 0) break
                                    total += count
                                    check(total <= WEB_DAV_MAX_ARCHIVE_BYTES) {
                                        "WebDAV backup exceeds 256 MiB"
                                    }
                                    output.write(buffer, 0, count)
                                }
                            }
                        }
                    }
                }
                try {
                    decryptFile(encryptedFile, archiveFile, encryptionPassword)
                } catch (error: AEADBadTagException) {
                    throw WebDavTransferException(
                        "Backup password is incorrect or the backup is corrupt",
                        error,
                    )
                }
                DownloadFileResult(archiveFile.length(), sha256(encryptedFile))
            } finally {
                encryptedFile.delete()
            }
        }
    }

    /** Deletes a generated backup file from WebDAV. */
    suspend fun delete(
        url: String,
        remotePath: String,
        username: String?,
        password: String?,
    ): kotlin.Result<Unit> = withContext(Dispatchers.IO) {
        runCatching {
            validatePath(url, remotePath)
            require(isBackupName(remotePath.substringAfterLast('/'))) {
                "Only generated WebDAV backups can be deleted"
            }
            val request = request(url, remotePath, username, password)
                .method("DELETE", null)
                .build()
            client.newCall(request).execute().use { response ->
                checkResponse(response, "delete WebDAV backup")
            }
        }
    }

    /** Lists generated backup files in a WebDAV folder. */
    suspend fun listBackups(
        url: String,
        remoteDirectory: String,
        username: String?,
        password: String?,
    ): kotlin.Result<List<RemoteBackup>> = withContext(Dispatchers.IO) {
        runCatching {
            validatePath(url, remoteDirectory)
            val request = request(url, remoteDirectory, username, password)
                .header("Depth", "1")
                .method("PROPFIND", PROPFIND_BODY)
                .build()
            client.newCall(request).execute().use { response ->
                checkResponse(response, "list WebDAV backups")
                val body = response.body ?: error("WebDAV backup list is empty")
                check(body.contentLength() <= MAX_LIST_BYTES) { "WebDAV backup list is too large" }
                val xml = body.string()
                check(xml.toByteArray(Charsets.UTF_8).size <= MAX_LIST_BYTES) { "WebDAV backup list is too large" }
                parseBackups(xml, url, remoteDirectory)
            }
        }
    }

    /** Verifies URL syntax, authentication, directory access, and write permission. */
    suspend fun testConnection(
        url: String,
        remoteDirectory: String,
        username: String?,
        password: String?,
    ): kotlin.Result<WebDavConnectionTestResult> = withContext(Dispatchers.IO) {
        runCatching {
            val checks = mutableListOf<WebDavCheck>()
            try {
                validatePath(url, remoteDirectory)
                checks += WebDavCheck(WebDavCheckStage.URL, true, "URL is valid")
            } catch (error: Exception) {
                checks += WebDavCheck(WebDavCheckStage.URL, false, error.message ?: "Invalid WebDAV URL")
                return@runCatching WebDavConnectionTestResult(checks)
            }

            val directoryRequest = request(url, remoteDirectory, username, password)
                .header("Depth", "0")
                .method("PROPFIND", PROPFIND_BODY)
                .build()
            val directoryError = runCatching {
                client.newCall(directoryRequest).execute().use { response ->
                    checkResponse(response, "check WebDAV directory")
                }
            }.exceptionOrNull()
            if (directoryError != null) {
                val authenticationFailed = (directoryError as? WebDavTransferException)?.httpCode in setOf(401, 403)
                checks += WebDavCheck(
                    WebDavCheckStage.AUTHENTICATION,
                    !authenticationFailed,
                    if (authenticationFailed) "Authentication failed" else "Authentication accepted or not required",
                )
                checks += WebDavCheck(
                    WebDavCheckStage.DIRECTORY,
                    false,
                    directoryError.message ?: "Remote directory is unavailable",
                )
                return@runCatching WebDavConnectionTestResult(checks)
            }
            checks += WebDavCheck(WebDavCheckStage.AUTHENTICATION, true, "Authentication accepted")
            checks += WebDavCheck(WebDavCheckStage.DIRECTORY, true, "Remote directory is accessible")

            val probePath = "$remoteDirectory/.clawseed-connection-test-${UUID.randomUUID()}.tmp"
            val writeError = runCatching {
                val probe = request(url, probePath, username, password)
                    .put(ByteArray(0).toRequestBody(BINARY_MEDIA_TYPE))
                    .build()
                client.newCall(probe).execute().use { response ->
                    checkResponse(response, "check WebDAV write permission")
                }
            }.exceptionOrNull()
            if (writeError != null) {
                checks += WebDavCheck(
                    WebDavCheckStage.WRITE,
                    false,
                    writeError.message ?: "Remote directory is not writable",
                )
            } else {
                val cleanupError = runCatching {
                    val delete = request(url, probePath, username, password)
                        .method("DELETE", null)
                        .build()
                    client.newCall(delete).execute().use { response ->
                        checkResponse(response, "clean up WebDAV connection test")
                    }
                }.exceptionOrNull()
                checks += WebDavCheck(
                    WebDavCheckStage.WRITE,
                    true,
                    if (cleanupError == null) "Remote directory is writable" else
                        "Remote directory is writable; test file cleanup failed",
                )
            }
            WebDavConnectionTestResult(checks)
        }
    }

    private fun ensureCollections(url: String, remotePath: String, username: String?, password: String?) {
        val parent = remotePath.substringBeforeLast('/', "")
        if (parent.isEmpty()) return
        var current = ""
        parent.split('/').forEach { segment ->
            current = if (current.isEmpty()) segment else "$current/$segment"
            val request = request(url, current, username, password).method("MKCOL", null).build()
            withRetry {
                client.newCall(request).execute().use { response ->
                    if (!response.isSuccessful && response.code != 405 && response.code != 409) {
                        throw responseFailure("create WebDAV collection", response.code)
                    }
                }
            }
        }
    }

    private fun request(url: String, path: String, username: String?, password: String?) =
        Request.Builder().url(resolve(url, path)).withAuth(username, password)

    private fun Request.Builder.withAuth(username: String?, password: String?): Request.Builder {
        if (!username.isNullOrBlank() && password != null) {
            header("Authorization", Credentials.basic(username, password))
        }
        return this
    }

    private fun validate(url: String, remotePath: String, encryptionPassword: String) {
        validatePath(url, remotePath)
        require(encryptionPassword.length >= 8) { "Encryption password must contain at least 8 characters" }
    }

    private fun validatePath(url: String, remotePath: String) {
        require(url.startsWith("https://") || url.startsWith("http://")) { "WebDAV URL must use HTTP(S)" }
        require(remotePath.isNotBlank() && remotePath.split('/').none { it.isBlank() || it == "." || it == ".." }) {
            "Remote path must be a non-empty relative path"
        }
    }

    private fun parseBackups(xml: String, baseUrl: String, remoteDirectory: String): List<RemoteBackup> {
        val parser = Xml.newPullParser().apply {
            setInput(StringReader(xml))
        }
        val backups = mutableListOf<RemoteBackup>()
        var current: MutableRemoteBackup? = null
        var event = parser.eventType
        while (event != XmlPullParser.END_DOCUMENT) {
            when (event) {
                XmlPullParser.START_TAG -> when (parser.name.substringAfter(':')) {
                    "response" -> current = MutableRemoteBackup()
                    "href" -> current?.path = parser.nextText()
                    "getcontentlength" -> current?.bytes = parser.nextText().toLongOrNull() ?: 0L
                    "getlastmodified" -> current?.lastModified = parser.nextText()
                    "collection" -> current?.isCollection = true
                }
                XmlPullParser.END_TAG -> if (parser.name.substringAfter(':') == "response") {
                    current?.let { item ->
                        val path = item.path?.let { toRemotePath(it, baseUrl) }
                        val name = path?.substringAfterLast('/')
                        if (!item.isCollection && path != null && name != null && isBackupName(name)) {
                            backups += RemoteBackup(name, path, item.bytes, item.lastModified)
                        }
                    }
                    current = null
                }
            }
            event = parser.next()
        }
        return backups.distinctBy { it.path }.sortedWith(
            compareByDescending<RemoteBackup> { it.lastModified.orEmpty() }.thenByDescending { it.name },
        )
    }

    private fun toRemotePath(href: String, baseUrl: String): String {
        val hrefPath = runCatching { URI(href).rawPath ?: href }.getOrDefault(href)
        val decodedPath = runCatching { URLDecoder.decode(hrefPath, Charsets.UTF_8.name()) }
            .getOrDefault(hrefPath)
            .trim('/')
        val basePath = runCatching { URI(baseUrl).path.trim('/') }.getOrDefault("")
        val relative = if (basePath.isNotEmpty() && decodedPath.startsWith("$basePath/")) {
            decodedPath.removePrefix("$basePath/")
        } else {
            decodedPath
        }
        return relative.trim('/')
    }

    private fun isBackupName(name: String): Boolean =
        name.startsWith("clawseed-") &&
            (name.endsWith(".zip", ignoreCase = true) || name.endsWith(".enc", ignoreCase = true))

    private class MutableRemoteBackup {
        var path: String? = null
        var bytes: Long = 0
        var lastModified: String? = null
        var isCollection: Boolean = false
    }

    private fun resolve(base: String, path: String): String =
        base.trimEnd('/') + "/" + path.trim('/').split('/').joinToString("/") {
            URLEncoder.encode(it, Charsets.UTF_8.name()).replace("+", "%20")
        }

    private fun encrypt(plain: ByteArray, password: String): ByteArray {
        val salt = ByteArray(SALT_BYTES)
        val iv = ByteArray(IV_BYTES)
        SecureRandom().nextBytes(salt)
        SecureRandom().nextBytes(iv)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(password, salt), GCMParameterSpec(128, iv))
        val ciphertext = cipher.doFinal(plain)
        return ByteArrayOutputStream(MAGIC.size + salt.size + iv.size + ciphertext.size).apply {
            write(MAGIC)
            write(salt)
            write(iv)
            write(ciphertext)
        }.toByteArray()
    }

    private fun encryptFile(inputFile: File, outputFile: File, password: String) {
        val salt = ByteArray(SALT_BYTES)
        val iv = ByteArray(IV_BYTES)
        SecureRandom().nextBytes(salt)
        SecureRandom().nextBytes(iv)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(password, salt), GCMParameterSpec(128, iv))
        BufferedOutputStream(FileOutputStream(outputFile)).use { rawOutput ->
            rawOutput.write(MAGIC)
            rawOutput.write(salt)
            rawOutput.write(iv)
            CipherOutputStream(rawOutput, cipher).use { encryptedOutput ->
                BufferedInputStream(FileInputStream(inputFile)).use { input ->
                    input.copyTo(encryptedOutput, DEFAULT_BUFFER_SIZE)
                }
            }
        }
    }

    private fun decrypt(payload: ByteArray, password: String): ByteArray {
        require(payload.size > MAGIC.size + SALT_BYTES + IV_BYTES) { "WebDAV backup is invalid" }
        require(payload.copyOfRange(0, MAGIC.size).contentEquals(MAGIC)) { "Unsupported WebDAV backup format" }
        val saltStart = MAGIC.size
        val ivStart = saltStart + SALT_BYTES
        val salt = payload.copyOfRange(saltStart, ivStart)
        val iv = payload.copyOfRange(ivStart, ivStart + IV_BYTES)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(password, salt), GCMParameterSpec(128, iv))
        return cipher.doFinal(payload.copyOfRange(ivStart + IV_BYTES, payload.size))
    }

    private fun decryptFile(inputFile: File, outputFile: File, password: String) {
        require(inputFile.length() > MAGIC.size + SALT_BYTES + IV_BYTES) {
            "WebDAV backup is invalid"
        }
        BufferedInputStream(FileInputStream(inputFile)).use { bufferedInput ->
            DataInputStream(bufferedInput).use { rawInput ->
                val magic = ByteArray(MAGIC.size)
                rawInput.readFully(magic)
                require(magic.contentEquals(MAGIC)) { "Unsupported WebDAV backup format" }
                val salt = ByteArray(SALT_BYTES)
                rawInput.readFully(salt)
                val iv = ByteArray(IV_BYTES)
                rawInput.readFully(iv)
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")
                cipher.init(Cipher.DECRYPT_MODE, key(password, salt), GCMParameterSpec(128, iv))
                CipherInputStream(rawInput, cipher).use { encryptedInput ->
                    BufferedOutputStream(FileOutputStream(outputFile)).use { output ->
                        encryptedInput.copyTo(output, DEFAULT_BUFFER_SIZE)
                    }
                }
            }
        }
    }

    private fun key(password: String, salt: ByteArray): SecretKeySpec {
        val spec: KeySpec = PBEKeySpec(password.toCharArray(), salt, PBKDF2_ITERATIONS, 256)
        val bytes = SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).encoded
        return SecretKeySpec(bytes, "AES")
    }

    private fun sha256(bytes: ByteArray): String =
        java.security.MessageDigest.getInstance("SHA-256").digest(bytes)
            .joinToString("") { "%02x".format(it) }

    private fun sha256(file: File): String {
        val digest = java.security.MessageDigest.getInstance("SHA-256")
        BufferedInputStream(FileInputStream(file)).use { input ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    private fun okhttp3.Call.executeChecked(operation: String): okhttp3.Response {
        val response = execute()
        checkResponse(response, operation)
        return response
    }

    private fun checkResponse(response: okhttp3.Response, operation: String) {
        if (!response.isSuccessful) throw responseFailure(operation, response.code)
    }

    private fun responseFailure(operation: String, code: Int): WebDavTransferException {
        val detail = when (code) {
            401, 403 -> "authentication failed"
            404 -> "remote path was not found"
            507 -> "remote storage is full"
            408, 425, 429 -> "the server is temporarily unavailable"
            in 500..599 -> "the server is temporarily unavailable"
            else -> "request failed"
        }
        val message = "$operation: $detail (HTTP $code)"
        return if (code == 408 || code == 425 || code == 429 || code in 500..599) {
            RetryableWebDavException(message, code)
        } else {
            WebDavTransferException(message, httpCode = code)
        }
    }

    private inline fun <T> withRetry(block: () -> T): T {
        var retry = 0
        while (true) {
            try {
                return block()
            } catch (error: IOException) {
                if (retry >= MAX_RETRIES || !isRetryable(error)) throw error
                retry++
                try {
                    Thread.sleep(RETRY_DELAYS_MS[retry - 1])
                } catch (interrupted: InterruptedException) {
                    Thread.currentThread().interrupt()
                    throw error
                }
            }
        }
    }

    private fun isRetryable(error: IOException): Boolean =
        error is RetryableWebDavException || error is SocketTimeoutException || error is SocketException || error is EOFException

    companion object {
        private val MAGIC = "CLAWSEED-WEBDAV-ANDROID-V1\n".toByteArray()
        private const val SALT_BYTES = 16
        private const val IV_BYTES = 12
        private const val PBKDF2_ITERATIONS = 150_000
        private const val MAX_LIST_BYTES = 1L * 1024L * 1024L
        private const val MAX_RETRIES = 2
        private val RETRY_DELAYS_MS = longArrayOf(250, 1_000)
        private val BINARY_MEDIA_TYPE = "application/octet-stream".toMediaType()
        private val PROPFIND_BODY = """
            <?xml version="1.0" encoding="utf-8" ?>
            <d:propfind xmlns:d="DAV:">
              <d:prop>
                <d:getcontentlength />
                <d:getlastmodified />
                <d:resourcetype />
              </d:prop>
            </d:propfind>
        """.trimIndent().toRequestBody("application/xml".toMediaType())

        private fun defaultClient(timeouts: WebDavTimeouts = WebDavTimeouts()): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(timeouts.connectSeconds, TimeUnit.SECONDS)
            .readTimeout(timeouts.readSeconds, TimeUnit.SECONDS)
            .writeTimeout(timeouts.writeSeconds, TimeUnit.SECONDS)
            .build()
    }
}
