package dev.clawseed.demo.ui.chat

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ImageAttachment
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
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
    val error: String? = null,
    val awaitingReply: Boolean = false,
)

/** Persistent app-owned copies survive provider URI permission expiry and process death. */
internal class ChatImageDrafts(private val context: Context) {
    private val directory = File(context.filesDir, "chat-image-drafts").apply { mkdirs() }
    private val prefs = context.getSharedPreferences("chat-image-drafts", Context.MODE_PRIVATE)
    private val json = Json { ignoreUnknownKeys = true }
    private val initial = runCatching {
        json.decodeFromString<Map<String, List<ChatImageDraft>>>(prefs.getString("drafts", "{}")!!)
            .mapValues { (_, images) -> images.map { it.copy(uploading = false, awaitingReply = false) } }
    }.getOrDefault(emptyMap())
    private val mutable = MutableStateFlow(initial)
    val drafts = mutable.asStateFlow()

    fun savedText(key: String): String = prefs.getString("text:$key", "").orEmpty()
    fun saveText(key: String, text: String) { prefs.edit().putString("text:$key", text).commit() }

    fun file(id: String): File = File(directory, "$id.png")

    fun update(key: String, images: List<ChatImageDraft>) {
        mutable.value = mutable.value.toMutableMap().apply {
            if (images.isEmpty()) remove(key) else put(key, images)
        }
        prefs.edit().putString("drafts", json.encodeToString(mutable.value)).commit()
    }

    fun replace(key: String, image: ChatImageDraft) {
        update(key, mutable.value[key].orEmpty().map { if (it.id == image.id) image else it })
    }

    fun remove(key: String, id: String) {
        update(key, mutable.value[key].orEmpty().filterNot { it.id == id })
        file(id).delete()
    }

    suspend fun copyImage(uri: Uri): ChatImageDraft = withContext(Dispatchers.IO) {
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
                    check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)) { "Unable to prepare image" }
                    output.toByteArray()
                }
                if (bytes.size <= 5 * 1024 * 1024) break
                check(bitmap.width > 512 && bitmap.height > 512) { "Image exceeds 5 MiB after processing" }
                val smaller = Bitmap.createScaledBitmap(bitmap, maxOf(1, bitmap.width * 3 / 4), maxOf(1, bitmap.height * 3 / 4), true)
                if (smaller !== bitmap) bitmap.recycle()
                bitmap = smaller
            }
            val draft = ChatImageDraft(UUID.randomUUID().toString())
            file(draft.id).outputStream().use { it.write(bytes); it.fd.sync() }
            draft
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
