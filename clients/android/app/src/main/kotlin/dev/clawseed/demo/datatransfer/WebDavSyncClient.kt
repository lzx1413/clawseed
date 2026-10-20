package dev.clawseed.demo.datatransfer

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.Credentials
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.ByteArrayOutputStream
import java.io.StringReader
import java.net.URI
import java.net.URLDecoder
import java.net.URLEncoder
import java.security.SecureRandom
import java.security.spec.KeySpec
import java.util.concurrent.TimeUnit
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.SecretKeySpec
import android.util.Xml
import org.xmlpull.v1.XmlPullParser

internal const val WEB_DAV_MAX_ARCHIVE_BYTES = 256L * 1024L * 1024L

/** Uploads and downloads an encrypted data-transfer archive through WebDAV. */
class WebDavSyncClient(private val client: OkHttpClient = defaultClient()) {
    data class UploadResult(val bytes: Int, val checksum: String)
    data class DownloadResult(val bytes: ByteArray, val checksum: String)
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
                check(response.isSuccessful) { "delete WebDAV backup failed (HTTP ${response.code})" }
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
                check(response.isSuccessful) { "list WebDAV backups failed (HTTP ${response.code})" }
                val body = response.body ?: error("WebDAV backup list is empty")
                check(body.contentLength() <= MAX_LIST_BYTES) { "WebDAV backup list is too large" }
                val xml = body.string()
                check(xml.toByteArray(Charsets.UTF_8).size <= MAX_LIST_BYTES) { "WebDAV backup list is too large" }
                parseBackups(xml, url, remoteDirectory)
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
        private const val MAX_LIST_BYTES = 1L * 1024L * 1024L
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

        private fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(180, TimeUnit.SECONDS)
            .writeTimeout(180, TimeUnit.SECONDS)
            .build()
    }
}
