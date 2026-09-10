package dev.clawseed.demo.ui.chat

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.util.AtomicFile
import dev.clawseed.sdk.core.model.FileAttachment
import dev.clawseed.sdk.core.model.FileAttachmentLimits as Limits
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File
import java.util.UUID

@Serializable
internal data class ChatFileDraft(
    val id: String,
    val name: String,
    val mime: String,
    val format: String,
    val size: Long = 0,
    val status: String = "importing",
    val error: String? = null,
    val source: String? = null,
    val referenced: Boolean = false,
    val sent: Boolean = false,
    val awaitingReply: Boolean = false,
)

/** A gateway/session digest is supplied by the actual session, never by tool arguments. */
internal class ChatFileAttachments(private val context: Context) {
    private val directory = File(context.filesDir, "chat-file-attachments")
    private val manifest = AtomicFile(File(directory, "manifest.json"))
    private val json = Json { ignoreUnknownKeys = true }
    private val mutex = Mutex()
    private val _entries = MutableStateFlow<Map<String, List<ChatFileDraft>>>(emptyMap())
    val entries = _entries.asStateFlow()
    private val _ready = MutableStateFlow(false)
    val ready = _ready.asStateFlow()

    suspend fun load() = withContext(Dispatchers.IO) { mutex.withLock {
        if (_ready.value) return@withLock
        directory.mkdirs()
        val loaded = if (manifest.baseFile.exists()) manifest.openRead().use {
            json.decodeFromString<Map<String, List<ChatFileDraft>>>(it.reader().readText())
        } else emptyMap()
        persist(loaded.mapValues { (_, list) -> list.map {
            if (it.status in setOf("importing", "parsing")) it.copy(status = "failed", error = "导入被中断，请重试", awaitingReply = false)
            else it.copy(awaitingReply = false)
        } })
        _ready.value = true
    } }

    private fun original(id: String): File { checkId(id); return File(directory, "$id.original") }
    internal fun hasOriginal(id: String): Boolean = original(id).isFile
    private fun cache(id: String): File { checkId(id); return File(directory, "$id.json") }
    private fun checkId(id: String) = require(Regex("file_[0-9a-f]{32}").matches(id)) { "附件 ID 无效" }
    private fun persist(value: Map<String, List<ChatFileDraft>>) {
        val stream = manifest.startWrite()
        try { stream.write(json.encodeToString(value).toByteArray()); manifest.finishWrite(stream) }
        catch (error: Exception) { manifest.failWrite(stream); throw error }
        _entries.value = value
    }

    private suspend fun change(key: String, block: (List<ChatFileDraft>) -> List<ChatFileDraft>) = withContext(Dispatchers.IO) {
        mutex.withLock { check(_ready.value) { "附件草稿尚未载入" }; persist(_entries.value + (key to block(_entries.value[key].orEmpty()))) }
    }

    suspend fun import(key: String, uri: Uri, attachmentId: String = "file_" + UUID.randomUUID().toString().replace("-", "")) = withContext(Dispatchers.IO) {
        checkId(attachmentId)
        if (entries.value[key].orEmpty().any { it.id == attachmentId }) return@withContext
        var name = uri.lastPathSegment?.substringAfterLast('/') ?: "文件"
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) name = it.getString(0) ?: name
        }
        require(name.length <= 255) { "文件名过长" }
        val format = name.substringAfterLast('.', "").lowercase()
        val mime = when (format) {
            "md" -> "text/markdown"
            "txt" -> "text/plain"
            "csv" -> "text/csv"
            "pdf" -> "application/pdf"
            "docx" -> "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            else -> error("支持 MD、TXT、CSV、文字 PDF、DOCX；旧 DOC 请另存为 DOCX")
        }
        runCatching { context.contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION) }
        val draft = ChatFileDraft(attachmentId, name, mime, format, source = uri.toString())
        change(key) {
            require(it.count { item -> !item.sent } < Limits.MAX_FILES) { "每条消息最多 4 个文件" }
            it + draft
        }
        prepare(key, draft)
    }

    suspend fun retry(key: String, id: String) {
        val draft = entries.value[key].orEmpty().find { it.id == id && !it.sent } ?: error("当前会话没有此附件")
        prepare(key, draft)
    }

    private suspend fun prepare(key: String, draft: ChatFileDraft) = withContext(Dispatchers.IO) {
        var current = draft.copy(error = null, status = "importing")
        try {
            change(key) { list -> list.map { if (it.id == draft.id) current else it } }
            if (!original(draft.id).exists()) {
                val uri = Uri.parse(checkNotNull(draft.source) { "原文件已丢失，请重新选择" })
                val temp = File(directory, "${draft.id}.copy")
                try {
                    context.contentResolver.openInputStream(uri).use { input ->
                        checkNotNull(input) { "无法打开文件，请重新选择" }
                        temp.outputStream().use { output ->
                            val buffer = ByteArray(8192)
                            var size = 0L
                            while (true) {
                                val n = input.read(buffer)
                                if (n < 0) break
                                size += n
                                require(size <= Limits.MAX_FILE_BYTES) { "单文件不能超过 20 MiB" }
                                val others = entries.value[key].orEmpty().filter { !it.sent && it.id != draft.id }.sumOf { it.size }
                                require(others + size <= Limits.MAX_IMPORT_BYTES) { "本条消息文件总量不能超过 50 MiB" }
                                output.write(buffer, 0, n)
                            }
                            require(size > 0) { "文件为空" }
                            output.fd.sync()
                        }
                    }
                    check(temp.renameTo(original(draft.id))) { "无法保存文件副本" }
                } finally { temp.delete() }
            }
            current = current.copy(size = original(draft.id).length(), status = "parsing")
            change(key) { list -> list.map { if (it.id == draft.id) current else it } }
            val parsed = when (draft.format) {
                "pdf" -> PdfAttachmentParser.parse(context, original(draft.id))
                "docx" -> DocxAttachmentParser.parse(original(draft.id))
                "csv" -> TextAttachmentParser.csv(original(draft.id).readBytes())
                else -> TextAttachmentParser.text(original(draft.id).readBytes())
            }
            val target = AtomicFile(cache(draft.id))
            val stream = target.startWrite()
            try { stream.write(json.encodeToString(parsed).toByteArray()); target.finishWrite(stream) }
            catch (error: Exception) { target.failWrite(stream); throw error }
            current = current.copy(status = "ready")
        } catch (error: Exception) {
            if (error is kotlinx.coroutines.CancellationException) throw error
            current = current.copy(status = "failed", error = error.message ?: "解析失败")
        }
        change(key) { list -> list.map { if (it.id == draft.id) current else it } }
    }

    suspend fun remove(key: String, id: String) {
        // Retain all source copies conservatively; a crash may have happened after server acceptance.
        change(key) { list -> list.filterNot { it.id == id && !it.referenced }.map { if (it.id == id) it.copy(sent = true) else it } }
    }

    suspend fun markSending(key: String, ids: Set<String>) = change(key) { list ->
        list.map { if (it.id in ids) it.copy(referenced = true, awaitingReply = true) else it }
    }

    suspend fun finish(key: String, ids: Set<String>, success: Boolean) = change(key) { list ->
        list.map { if (it.id in ids) it.copy(awaitingReply = false, sent = success) else it }
    }

    suspend fun reconcile(key: String, sentIds: Set<String>) = change(key) { list ->
        list.map { if (it.id in sentIds && it.referenced) it.copy(sent = true, awaitingReply = false) else it }
    }

    private fun parsed(id: String): ParsedAttachment {
        require(original(id).isFile && cache(id).isFile) { "附件原文件或解析缓存已丢失，请重新导入" }
        return json.decodeFromString<ParsedAttachment>(cache(id).readText()).validate()
    }

    suspend fun metadata(key: String): List<FileAttachment> = withContext(Dispatchers.IO) {
        var remaining = Limits.AUTO_MESSAGE_CHARS
        entries.value[key].orEmpty().filter { !it.sent && !it.awaitingReply }.map { draft ->
            require(draft.status == "ready") { "请等待文件解析完成，或移除失败的附件" }
            val parsed = parsed(draft.id)
            val budget = if (parsed.characterCount <= Limits.AUTO_FILE_CHARS) minOf(Limits.AUTO_FILE_CHARS, remaining) else minOf(Limits.PREVIEW_CHARS, remaining)
            val excerpt = parsed.read(draft.id, count = Limits.READ_CHARS, budget = budget, allowLargeUnit = false)
            remaining -= excerpt.content.length
            FileAttachment(draft.id, draft.name, draft.mime, draft.size, draft.format, unit = parsed.unit, total = parsed.total, excerpt = excerpt)
        }
    }

    suspend fun read(key: String, id: String, start: Int, count: Int) = withContext(Dispatchers.IO) {
        check(_ready.value) { "附件索引尚未载入，请稍后重试" }
        val draft = entries.value[key].orEmpty().find { it.id == id && it.referenced } ?: error("附件不属于当前会话或尚未发送")
        require(draft.status == "ready") { draft.error ?: "附件尚未解析完成" }
        parsed(id).read(id, start, count)
    }
}
