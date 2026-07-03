package dev.clawseed.demo.ui.persona

import android.content.Context
import android.net.Uri
import dev.clawseed.sdk.embedded.GatewayConfigManager
import java.io.File

object PersonaAvatarStorage {
    private const val AVATAR_SCHEME = "clawseed-persona-avatar"

    fun avatarDir(context: Context): File =
        File(GatewayConfigManager(context).configDir(), "persona_avatars")

    fun saveAvatar(context: Context, personaName: String, source: Uri): String? {
        val dir = avatarDir(context)
        dir.mkdirs()
        val target = File(dir, "${safeFileStem(personaName)}-${System.currentTimeMillis()}.png")
        return runCatching {
            context.contentResolver.openInputStream(source)?.use { input ->
                target.outputStream().use { output ->
                    input.copyTo(output)
                }
            } ?: return null
            Uri.Builder()
                .scheme(AVATAR_SCHEME)
                .path(target.name)
                .build()
                .toString()
        }.getOrNull()
    }

    fun ensureLocalAvatar(context: Context, personaName: String, avatar: String): String {
        val trimmed = avatar.trim()
        if (!trimmed.startsWith("content://")) return avatar
        return saveAvatar(context, personaName, Uri.parse(trimmed)) ?: avatar
    }

    fun resolveAvatarUri(context: Context, avatar: String): Uri? {
        val uri = Uri.parse(avatar)
        if (uri.scheme != AVATAR_SCHEME) return uri
        val filename = uri.lastPathSegment ?: return null
        return Uri.fromFile(File(avatarDir(context), filename))
    }

    fun isAvatarUri(value: String): Boolean {
        return value.startsWith("content://") ||
            value.startsWith("file://") ||
            value.startsWith("$AVATAR_SCHEME:")
    }

    private fun safeFileStem(value: String): String {
        val stem = value.trim()
            .lowercase()
            .replace(Regex("[^a-z0-9._-]+"), "-")
            .trim('-')
        return stem.ifBlank { "persona" }.take(48)
    }
}
