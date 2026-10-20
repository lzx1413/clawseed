package dev.clawseed.demo.datatransfer

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Credentials
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.ByteArrayOutputStream
import java.net.URLEncoder
import java.security.SecureRandom
import java.security.spec.KeySpec
import java.util.concurrent.TimeUnit
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.SecretKeySpec

internal const val WEB_DAV_MAX_ARCHIVE_BYTES = 256L * 1024L * 1024L

/** Uploads and downloads an encrypted data-transfer archive through WebDAV. */
class WebDavSyncClient(private val client: OkHttpClient = defaultClient()) {
    data class UploadResult(val bytes: Int, val checksum: String)
    data class DownloadResult(val bytes: ByteArray, val checksum: String)

    suspend fun upload(
        url: String,
        remotePath: String,
        archive: ByteArray,
        username: String?,
        password: String?,
        encryptionPassword: String,
    ): kotlin.Result<UploadResult> = withContext(Dispatchers.IO) {
        runCatching {
            validate(url, remotePath, encryptionPassword)
            val encrypted = encrypt(archive, encryptionPassword)
            require(encrypted.size.toLong() <= WEB_DAV_MAX_ARCHIVE_BYTES) {
                "WebDAV backup exceeds 256 MiB"
            }
            ensureCollections(url, remotePath, username, password)
            val temporaryPath = "$remotePath.tmp-${System.currentTimeMillis()}"
            val upload = request(url, temporaryPath, username, password)
                .put(encrypted.toRequestBody(BINARY_MEDIA_TYPE))
                .build()
            client.newCall(upload).executeChecked("upload WebDAV backup").close()
            val move = Request.Builder()
                .url(resolve(url, temporaryPath))
                .header("Destination", resolve(url, remotePath))
                .header("Overwrite", "T")
                .method("MOVE", null)
                .withAuth(username, password)
                .build()
            client.newCall(move).execute().use { response ->
                check(response.isSuccessful) { "commit WebDAV backup failed (HTTP ${response.code})" }
            }
            UploadResult(encrypted.size, sha256(encrypted))
        }
    }

    suspend fun download(
        url: String,
        remotePath: String,
        username: String?,
        password: String?,
        encryptionPassword: String,
    ): kotlin.Result<DownloadResult> = withContext(Dispatchers.IO) {
        runCatching {
            validate(url, remotePath, encryptionPassword)
            val request = request(url, remotePath, username, password).build()
            client.newCall(request).execute().use { response ->
                check(response.isSuccessful) { "download WebDAV backup failed (HTTP ${response.code})" }
                val body = response.body ?: error("WebDAV backup is empty")
                check(body.contentLength() <= WEB_DAV_MAX_ARCHIVE_BYTES) {
                    "WebDAV backup exceeds 256 MiB"
                }
                val payload = body.bytes()
                check(payload.size.toLong() <= WEB_DAV_MAX_ARCHIVE_BYTES) {
                    "WebDAV backup exceeds 256 MiB"
                }
                DownloadResult(decrypt(payload, encryptionPassword), sha256(payload))
            }
        }
    }

    private fun ensureCollections(url: String, remotePath: String, username: String?, password: String?) {
        val parent = remotePath.substringBeforeLast('/', "")
        if (parent.isEmpty()) return
        var current = ""
        parent.split('/').forEach { segment ->
            current = if (current.isEmpty()) segment else "$current/$segment"
            val request = request(url, current, username, password).method("MKCOL", null).build()
            client.newCall(request).execute().use { response ->
                check(response.isSuccessful || response.code == 405 || response.code == 409) {
                    "create WebDAV collection failed (HTTP ${response.code})"
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
        require(url.startsWith("https://") || url.startsWith("http://")) { "WebDAV URL must use HTTP(S)" }
        require(remotePath.isNotBlank() && remotePath.split('/').none { it.isBlank() || it == "." || it == ".." }) {
            "Remote path must be a non-empty relative path"
        }
        require(encryptionPassword.length >= 8) { "Encryption password must contain at least 8 characters" }
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

    private fun key(password: String, salt: ByteArray): SecretKeySpec {
        val spec: KeySpec = PBEKeySpec(password.toCharArray(), salt, PBKDF2_ITERATIONS, 256)
        val bytes = SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).encoded
        return SecretKeySpec(bytes, "AES")
    }

    private fun sha256(bytes: ByteArray): String =
        java.security.MessageDigest.getInstance("SHA-256").digest(bytes)
            .joinToString("") { "%02x".format(it) }

    private fun okhttp3.Call.executeChecked(operation: String): okhttp3.Response {
        val response = execute()
        check(response.isSuccessful) { "$operation failed (HTTP ${response.code})" }
        return response
    }

    companion object {
        private val MAGIC = "CLAWSEED-WEBDAV-ANDROID-V1\n".toByteArray()
        private const val SALT_BYTES = 16
        private const val IV_BYTES = 12
        private const val PBKDF2_ITERATIONS = 150_000
        private val BINARY_MEDIA_TYPE = "application/octet-stream".toMediaType()

        private fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(180, TimeUnit.SECONDS)
            .writeTimeout(180, TimeUnit.SECONDS)
            .build()
    }
}
