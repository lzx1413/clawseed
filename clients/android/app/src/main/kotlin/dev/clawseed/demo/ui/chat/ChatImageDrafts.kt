package dev.clawseed.demo.ui.chat

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ImageAttachment
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.ByteArrayOutputStream
import java.io.File
import java.security.MessageDigest
import java.util.UUID

internal data class ImageDraftTarget(val key: String, val sessionId: String, val gateway: GatewayClient)

@Serializable
internal data class ChatImageDraft(
    val id: String,
    val attachment: ImageAttachment? = null,
    val uploading: Boolean = false,
    val preparing: Boolean = false,
    val error: String? = null,
    val awaitingReply: Boolean = false,
)

/** Persistent app-owned copies survive provider URI permission expiry and process death. */
internal class ChatImageDrafts(private val context: Context) {
    private val directory = File(context.filesDir, "chat-image-drafts")
    private val prefs by lazy { context.getSharedPreferences("chat-image-drafts", Context.MODE_PRIVATE) }
    private val json = Json { ignoreUnknownKeys = true }
    private val persistence = ImageDraftPersistence(
        read = {
            val images = runCatching {
                json.decodeFromString<Map<String, List<ChatImageDraft>>>(prefs.getString("drafts", "{}")!!)
            }.getOrDefault(emptyMap())
            val texts = prefs.all.mapNotNull { (key, value) ->
                if (key.startsWith("text:") && value is String) key.removePrefix("text:") to value else null
            }.toMap()
            ImageDraftSnapshot(images, texts)
        },
        write = { snapshot ->
            val editor = prefs.edit().clear().putString("drafts", json.encodeToString(snapshot.images))
            snapshot.texts.forEach { (key, text) -> editor.putString("text:$key", text) }
            check(editor.commit()) { "Unable to save image draft" }
        },
    )
    val drafts = persistence.drafts
    val ready = persistence.ready
    suspend fun load() = persistence.load()
    fun savedText(key: String): String = persistence.savedText(key)
    fun file(id: String): File = listOf("jpg", "png", "source")
        .map { File(directory, "$id.$it") }.firstOrNull { it.exists() } ?: File(directory, "$id.png")
    fun needsPreparation(id: String): Boolean = File(directory, "$id.source").exists()

    suspend fun saveText(key: String, text: String) = persistence.update {
        it.copy(texts = if (text.isEmpty()) it.texts - key else it.texts + (key to text))
    }

    suspend fun update(key: String, images: List<ChatImageDraft>) = transform(key) { images }

    suspend fun transform(key: String, change: (List<ChatImageDraft>) -> List<ChatImageDraft>) = persistence.update {
        val images = change(it.images[key].orEmpty())
        it.copy(images = if (images.isEmpty()) it.images - key else it.images + (key to images))
    }

    suspend fun replace(key: String, image: ChatImageDraft) = transform(key) { images ->
        images.map { if (it.id == image.id) image else it }
    }

    suspend fun remove(key: String, id: String) = removeAll(key, setOf(id))

    suspend fun removeAll(key: String, ids: Set<String>) {
        persistence.update {
            val images = it.images[key].orEmpty().filterNot { image -> image.id in ids }
            it.copy(
                images = if (images.isEmpty()) it.images - key else it.images + (key to images),
                texts = if (images.isEmpty()) it.texts - key else it.texts,
            )
        }
        withContext(Dispatchers.IO) { ids.forEach { id -> listOf("jpg", "png", "source", "tmp").forEach { File(directory, "$id.$it").delete() } } }
    }

    /** Save the original first so a preview is available before decoding or network I/O. */
    suspend fun stageImage(uri: Uri): ChatImageDraft = withContext(Dispatchers.IO) {
        directory.mkdirs()
        val draft = ChatImageDraft(UUID.randomUUID().toString(), preparing = true)
        val original = File(directory, "${draft.id}.source")
        try {
            context.contentResolver.openInputStream(uri).use { input ->
                checkNotNull(input) { "无法读取图片" }
                original.outputStream().use { output ->
                    val buffer = ByteArray(64 * 1024)
                    var total = 0L
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        total += count
                        require(total <= 50L * 1024 * 1024) { "原始图片不能超过 50 MiB" }
                        output.write(buffer, 0, count)
                    }
                    output.fd.sync()
                }
            }
            draft
        } catch (error: Exception) {
            original.delete()
            throw error
        }
    }

    suspend fun prepareImage(draft: ChatImageDraft): ChatImageDraft = withContext(Dispatchers.IO) {
        val source = File(directory, "${draft.id}.source")
        val uri = Uri.fromFile(source)
        val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeFile(source.path, bounds)
        val jpeg = bounds.outMimeType == "image/jpeg"
        // ImageDecoder applies EXIF orientation. Keep screenshot text lossless;
        // downscale only when dimensions or encoded size require it.
        var bitmap = if (android.os.Build.VERSION.SDK_INT >= 28) {
            ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                val scale = minOf(1.0, 4096.0 / maxOf(info.size.width, info.size.height))
                decoder.setTargetSize(maxOf(1, (info.size.width * scale).toInt()), maxOf(1, (info.size.height * scale).toInt()))
            }
        } else {
            decodeLegacyImage(context, uri)
        }
        try {
            var bytes: ByteArray
            while (true) {
                bytes = ByteArrayOutputStream().use { output ->
                    check(bitmap.compress(if (jpeg) Bitmap.CompressFormat.JPEG else Bitmap.CompressFormat.PNG, if (jpeg) 90 else 100, output)) { "Unable to prepare image" }
                    output.toByteArray()
                }
                if (bytes.size <= 5 * 1024 * 1024) break
                check(bitmap.width > 512 && bitmap.height > 512) { "Image exceeds 5 MiB after processing" }
                val smaller = Bitmap.createScaledBitmap(bitmap, maxOf(1, bitmap.width * 3 / 4), maxOf(1, bitmap.height * 3 / 4), true)
                if (smaller !== bitmap) bitmap.recycle()
                bitmap = smaller
            }
            val output = File(directory, "${draft.id}.${if (jpeg) "jpg" else "png"}")
            val temporary = File(directory, "${draft.id}.tmp")
            try {
                temporary.outputStream().use { it.write(bytes); it.fd.sync() }
                check(temporary.renameTo(output)) { "无法保存处理后的图片" }
                source.delete()
            } finally { temporary.delete() }
            draft.copy(preparing = false, error = null)
        } finally { bitmap.recycle() }
    }

    companion object {
        fun key(gateway: GatewayClient, sessionId: String): String {
            val source = "${gateway.baseUrl.trimEnd('/')}\n$sessionId"
            return MessageDigest.getInstance("SHA-256").digest(source.toByteArray()).joinToString("") { "%02x".format(it) }
        }
    }
}

private fun decodeLegacyImage(context: Context, uri: Uri): Bitmap {
    val options = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
    context.contentResolver.openInputStream(uri).use { android.graphics.BitmapFactory.decodeStream(it, null, options) }
    require(options.outWidth > 0 && options.outHeight > 0) { "Unsupported image" }
    options.inSampleSize = 1
    while (maxOf(options.outWidth, options.outHeight) / options.inSampleSize > 4096) options.inSampleSize *= 2
    options.inJustDecodeBounds = false
    val bitmap = context.contentResolver.openInputStream(uri).use {
        android.graphics.BitmapFactory.decodeStream(it, null, options)
    } ?: error("Unable to decode image")
    val orientation = context.contentResolver.openInputStream(uri).use { input ->
        androidx.exifinterface.media.ExifInterface(checkNotNull(input)).getAttributeInt(
            androidx.exifinterface.media.ExifInterface.TAG_ORIENTATION, androidx.exifinterface.media.ExifInterface.ORIENTATION_NORMAL)
    }
    val matrix = android.graphics.Matrix().apply {
        when (orientation) {
            2 -> setScale(-1f, 1f)
            3 -> setRotate(180f)
            4 -> setScale(1f, -1f)
            5 -> { setRotate(90f); postScale(-1f, 1f) }
            6 -> setRotate(90f)
            7 -> { setRotate(270f); postScale(-1f, 1f) }
            8 -> setRotate(270f)
        }
    }
    return Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true).also {
        if (it !== bitmap) bitmap.recycle()
    }
}
