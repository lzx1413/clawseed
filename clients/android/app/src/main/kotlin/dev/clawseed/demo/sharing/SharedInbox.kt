package dev.clawseed.demo.sharing

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.util.AtomicFile
import androidx.core.content.IntentCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File
import java.util.UUID
import kotlin.coroutines.coroutineContext

@Serializable
internal data class SharedItem(val id: String, val name: String, val image: Boolean, val index: Int)

@Serializable
internal data class SharedBundle(
    val id: String,
    val text: String,
    val items: List<SharedItem>,
    val warnings: List<String> = emptyList(),
    val gatewayKey: String? = null,
    val imported: Boolean = false,
)

/** Copies temporary provider grants before navigating or contacting a gateway. */
internal class SharedInbox(private val context: Context) {
    private val root = File(context.filesDir, "shared-inbox")
    private val json = Json { ignoreUnknownKeys = true }

    private fun directory(id: String): File {
        require(UUID.fromString(id).toString() == id) { "分享标识无效" }
        return File(root, id)
    }

    private fun read(id: String): SharedBundle? {
        val manifest = AtomicFile(File(directory(id), "manifest.json"))
        return if (manifest.baseFile.exists()) manifest.openRead().use { json.decodeFromString<SharedBundle>(it.reader().readText()) } else null
    }

    private fun write(bundle: SharedBundle) {
        val directory = directory(bundle.id).apply { mkdirs() }
        val manifest = AtomicFile(File(directory, "manifest.json"))
        val stream = manifest.startWrite()
        try { stream.write(json.encodeToString(bundle).toByteArray()); manifest.finishWrite(stream) }
        catch (error: Exception) { manifest.failWrite(stream); throw error }
    }

    suspend fun load(id: String): SharedBundle? = withContext(Dispatchers.IO) {
        // Ordinary gateway session IDs need not be UUIDs and have no inbox entry.
        if (runCatching { UUID.fromString(id).toString() == id }.getOrDefault(false)) mutex.withLock { read(id) } else null
    }

    suspend fun pending(): SharedBundle? = withContext(Dispatchers.IO) { mutex.withLock {
        root.listFiles().orEmpty().filter { it.isDirectory }
            .sortedByDescending { it.lastModified() }
            .firstNotNullOfOrNull { runCatching { read(it.name)?.takeUnless { bundle -> bundle.imported } }.getOrNull() }
    } }

    suspend fun bind(id: String, gatewayKey: String): SharedBundle = withContext(Dispatchers.IO) { mutex.withLock {
        val bundle = checkNotNull(read(id)) { "分享副本已丢失，请重新分享" }
        check(bundle.gatewayKey == null || bundle.gatewayKey == gatewayKey) { "请切回接收此分享时的网关" }
        bundle.copy(gatewayKey = gatewayKey).also { write(it) }
    } }

    fun file(bundle: SharedBundle, item: SharedItem): File {
        require(item.name == safeName(item.name) && item.index in 0..7)
        return File(File(directory(bundle.id), item.index.toString()), item.name)
    }

    suspend fun complete(id: String) = withContext(Dispatchers.IO) { mutex.withLock {
        val bundle = checkNotNull(read(id))
        // The small receipt prevents replay even after the temporary copies are removed.
        write(bundle.copy(imported = true, text = "", warnings = emptyList()))
        bundle.items.forEach { file(bundle, it).parentFile?.deleteRecursively() }
    } }

    suspend fun receive(id: String, intent: Intent): SharedBundle = withContext(Dispatchers.IO) { mutex.withLock {
        read(id)?.let { return@withLock it }
        require(isShare(intent)) { "不是系统分享请求" }
        val uris = sharedUris(intent)
        require(uris.size <= 8) { "一次最多分享 4 张图片和 4 个文件" }
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString().orEmpty()
        require(text.length <= 100_000) { "分享文字过长，请改为 TXT 文件" }
        val items = mutableListOf<SharedItem>()
        val warnings = mutableListOf<String>()
        var totalBytes = 0L
        var documentBytes = 0L
        for ((index, uri) in uris.withIndex()) {
            coroutineContext.ensureActive()
            var destination: File? = null
            try {
                // Never turn a foreign Intent into access to our private FileProvider.
                require(uri.scheme == "content" && uri.authority.orEmpty().substringAfter('@') != "${context.packageName}.fileprovider") { "分享链接无效，请从相册或文件管理器重新分享" }
                val mime = context.contentResolver.getType(uri)?.lowercase() ?: intent.type.orEmpty().lowercase()
                var name = ""
                context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) name = cursor.getString(0).orEmpty()
                }
                val extension = name.substringAfterLast('.', "").lowercase()
                val image = mime.startsWith("image/") || extension in setOf("jpg", "jpeg", "png", "webp", "heic", "heif", "gif", "bmp")
                if (!image && extension !in documents) {
                    val suffix = mimeExtensions[mime] ?: error("不支持此文件，请分享图片、PDF、MD、TXT、CSV 或 DOCX")
                    name = name.ifBlank { "分享文件" } + "." + suffix
                }
                name = safeName(name.ifBlank { if (image) "分享图片" else "分享文件" })
                require(items.count { it.image == image } < 4) { if (image) "每条消息最多 4 张图片" else "每条消息最多 4 个文件" }
                val itemId = UUID.nameUUIDFromBytes("$id:$index".toByteArray()).toString()
                val item = SharedItem(itemId, name, image, index)
                destination = file(SharedBundle(id, text, emptyList()), item)
                destination.parentFile?.mkdirs()
                var size = 0L
                context.contentResolver.openInputStream(uri).use { input ->
                    checkNotNull(input) { "无法读取分享内容，请重新分享" }
                    destination.outputStream().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            coroutineContext.ensureActive()
                            val n = input.read(buffer)
                            if (n < 0) break
                            size += n
                            require(size <= (if (image) 50L else 20L) * 1024 * 1024) { "分享文件过大：图片原图最多 50 MiB，文件最多 20 MiB" }
                            require(totalBytes + size <= 100L * 1024 * 1024) { "本次分享总量不能超过 100 MiB" }
                            require(image || documentBytes + size <= 50L * 1024 * 1024) { "本次分享文件总量不能超过 50 MiB" }
                            output.write(buffer, 0, n)
                        }
                        require(size > 0) { "分享文件为空" }
                        output.fd.sync()
                    }
                }
                totalBytes += size
                if (!image) documentBytes += size
                items += item
            } catch (error: Exception) {
                destination?.delete()
                if (error is kotlinx.coroutines.CancellationException) throw error
                warnings += "第 ${index + 1} 项：${error.message ?: "读取失败，请重新分享"}"
            }
        }
        require(items.isNotEmpty() || (uris.isEmpty() && text.isNotBlank())) { warnings.joinToString("\n").ifBlank { "没有收到图片或文件，请重新分享" } }
        SharedBundle(id, text, items, warnings).also { write(it) }
    } }

    companion object {
        private val mutex = Mutex()
        private val documents = setOf("md", "txt", "csv", "pdf", "docx")
        private val mimeExtensions = mapOf("application/pdf" to "pdf", "text/plain" to "txt", "text/markdown" to "md", "text/csv" to "csv", "application/vnd.openxmlformats-officedocument.wordprocessingml.document" to "docx")
        fun isShare(intent: Intent) = intent.action == Intent.ACTION_SEND || intent.action == Intent.ACTION_SEND_MULTIPLE
        fun safeName(name: String) = name.replace(Regex("[\\p{Cntrl}/\\\\]"), "_").take(240).takeUnless { it.isBlank() || it == "." || it == ".." } ?: "分享文件"
        fun sharedUris(intent: Intent): List<Uri> {
            val uris = linkedSetOf<Uri>()
            if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let { uris.addAll(it) }
            } else IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let { uris.add(it) }
            intent.clipData?.let { clip ->
                require(clip.itemCount <= 8) { "一次最多分享 8 个附件" }
                repeat(clip.itemCount) { clip.getItemAt(it).uri?.let(uris::add) }
            }
            return uris.toList()
        }
    }
}
