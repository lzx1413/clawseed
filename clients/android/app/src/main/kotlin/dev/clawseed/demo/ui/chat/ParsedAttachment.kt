package dev.clawseed.demo.ui.chat

import dev.clawseed.sdk.core.model.AttachmentReadResult
import dev.clawseed.sdk.core.model.FileAttachmentLimits
import kotlinx.serialization.Serializable

/** Immutable parse cache. Records and pages remain indivisible source units. */
@Serializable
internal data class ParsedAttachment(val unit: String, val parts: List<String>) {
    val total: Int get() = if (unit == "character") parts.single().codePointCount(0, parts.single().length) else parts.size
    val characterCount: Int get() = parts.sumOf { it.codePointCount(0, it.length) }

    fun validate(): ParsedAttachment {
        require(unit in setOf("character", "record", "page")) { "未知附件分段类型" }
        require(unit != "character" || parts.size == 1) { "文本缓存损坏" }
        require(parts.sumOf { it.length.toLong() } <= FileAttachmentLimits.MAX_PARSED_CHARS) { "解析内容超过 200 万字符限制" }
        require(unit == "character" || parts.all { it.length <= FileAttachmentLimits.MAX_UNIT_CHARS }) { "单条记录或单页超过 65536 字符限制" }
        require(parts.any { it.isNotBlank() }) { "文件没有可读取的文字" }
        return this
    }

    fun read(id: String, start: Int = 0, count: Int = if (unit == "character") FileAttachmentLimits.READ_CHARS else 10,
             budget: Int = FileAttachmentLimits.READ_CHARS, allowLargeUnit: Boolean = true): AttachmentReadResult {
        require(start in 0..total && count > 0 && count <= FileAttachmentLimits.READ_CHARS) { "读取范围无效：start 为从 0 开始的位置，count 为 1..16000" }
        if (unit == "character") {
            val text = parts.single()
            var end = minOf(total, start + minOf(count, budget))
            // Budgets are conservative UTF-16 sizes on Android; offsets remain code points.
            val from = text.offsetByCodePoints(0, start)
            while (text.offsetByCodePoints(0, end) - from > budget) end--
            return AttachmentReadResult(id, unit, start, end, total, end == total,
                text.substring(from, text.offsetByCodePoints(0, end)))
        }
        var end = start
        val output = StringBuilder()
        while (end < total && end - start < count) {
            val item = "[${unit} ${end + 1}]\n${parts[end]}\n"
            if (output.length + item.length > budget && (end != start || !allowLargeUnit)) break
            output.append(item)
            end++
        }
        return AttachmentReadResult(id, unit, start, end, total, end == total, output.toString())
    }
}
