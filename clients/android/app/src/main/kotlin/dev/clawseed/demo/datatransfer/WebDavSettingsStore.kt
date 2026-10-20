package dev.clawseed.demo.datatransfer

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.nio.ByteBuffer
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

data class WebDavSettings(
    val url: String = "",
    val remoteDirectory: String = "clawseed",
    val lastRemoteFile: String = "",
    val username: String = "",
    val password: String = "",
    val rememberPassword: Boolean = false,
)

/** Persists WebDAV connection settings and optionally encrypts its password with Keystore. */
class WebDavSettingsStore(context: Context) {
    private val preferences = context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun load(): WebDavSettings {
        val rememberPassword = preferences.getBoolean(KEY_REMEMBER_PASSWORD, false)
        val encryptedPassword = preferences.getString(KEY_PASSWORD, null)
        val hasDirectoryKey = preferences.contains(KEY_REMOTE_DIRECTORY)
        val storedPath = preferences.getString(
            if (hasDirectoryKey) KEY_REMOTE_DIRECTORY else KEY_REMOTE_PATH,
            DEFAULT_REMOTE_DIRECTORY,
        ).orEmpty()
        val legacyFile = if (hasDirectoryKey) {
            null
        } else {
            storedPath.substringAfterLast('/').takeIf {
                it.endsWith(".zip", ignoreCase = true) || it.endsWith(".enc", ignoreCase = true)
            }
        }
        val directory = if (legacyFile != null) storedPath.substringBeforeLast('/', "") else storedPath
        val legacyRemoteFile = legacyFile?.let { storedPath.trim('/') }
        return WebDavSettings(
            url = preferences.getString(KEY_URL, "").orEmpty(),
            remoteDirectory = normalizeDirectory(directory),
            lastRemoteFile = preferences.getString(KEY_LAST_REMOTE_FILE, null) ?: legacyRemoteFile.orEmpty(),
            username = preferences.getString(KEY_USERNAME, "").orEmpty(),
            password = if (rememberPassword && encryptedPassword != null) {
                runCatching { decrypt(encryptedPassword) }.getOrDefault("")
            } else {
                ""
            },
            rememberPassword = rememberPassword,
        )
    }

    fun save(settings: WebDavSettings) {
        preferences.edit()
            .putString(KEY_URL, settings.url.trim())
            .putString(
                KEY_REMOTE_DIRECTORY,
                normalizeDirectory(settings.remoteDirectory),
            )
            .putString(KEY_LAST_REMOTE_FILE, settings.lastRemoteFile.trim())
            .putString(KEY_USERNAME, settings.username)
            .putBoolean(KEY_REMEMBER_PASSWORD, settings.rememberPassword)
            .remove(KEY_REMOTE_PATH)
            .apply {
                if (settings.rememberPassword && settings.password.isNotEmpty()) {
                    putString(KEY_PASSWORD, encrypt(settings.password))
                } else {
                    remove(KEY_PASSWORD)
                }
            }
            .apply()
    }

    private fun encrypt(value: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val ciphertext = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val payload = ByteBuffer.allocate(cipher.iv.size + ciphertext.size)
            .put(cipher.iv)
            .put(ciphertext)
            .array()
        return Base64.encodeToString(payload, Base64.NO_WRAP)
    }

    private fun decrypt(value: String): String {
        val payload = Base64.decode(value, Base64.NO_WRAP)
        require(payload.size > GCM_IV_BYTES)
        val iv = payload.copyOfRange(0, GCM_IV_BYTES)
        val ciphertext = payload.copyOfRange(GCM_IV_BYTES, payload.size)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(GCM_TAG_BITS, iv))
        return cipher.doFinal(ciphertext).toString(Charsets.UTF_8)
    }

    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setUserAuthenticationRequired(false)
                .build(),
        )
        return generator.generateKey()
    }

    companion object {
        private const val PREFERENCES_NAME = "webdav_settings"
        private const val KEY_ALIAS = "clawseed_webdav_password"
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val GCM_IV_BYTES = 12
        private const val GCM_TAG_BITS = 128
        private const val KEY_URL = "url"
        private const val KEY_REMOTE_PATH = "remote_path"
        private const val KEY_REMOTE_DIRECTORY = "remote_directory"
        private const val KEY_LAST_REMOTE_FILE = "last_remote_file"
        private const val KEY_USERNAME = "username"
        private const val KEY_PASSWORD = "password"
        private const val KEY_REMEMBER_PASSWORD = "remember_password"
        private const val DEFAULT_REMOTE_DIRECTORY = "clawseed"

        fun normalizeDirectory(value: String): String {
            val trimmed = value.trim().trim('/')
            if (trimmed.isEmpty()) return DEFAULT_REMOTE_DIRECTORY
            val lastSegment = trimmed.substringAfterLast('/')
            if (lastSegment.endsWith(".zip", ignoreCase = true) ||
                lastSegment.endsWith(".enc", ignoreCase = true)
            ) {
                return trimmed.substringBeforeLast('/', "").trim('/').ifEmpty { DEFAULT_REMOTE_DIRECTORY }
            }
            return trimmed
        }
    }
}
