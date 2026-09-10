package dev.clawseed.demo.ui.chat

import dev.clawseed.sdk.core.model.FileAttachmentLimits
import java.nio.ByteBuffer
import java.nio.charset.Charset
import java.nio.charset.CodingErrorAction

internal object TextAttachmentParser {
    /** Only unambiguous Unicode encodings are accepted; legacy encodings must be converted. */
    fun decode(bytes: ByteArray, maxChars: Int = FileAttachmentLimits.MAX_PARSED_CHARS): String {
        val (charset, skip) = when {
            bytes.size >= 3 && bytes[0] == 0xef.toByte() && bytes[1] == 0xbb.toByte() && bytes[2] == 0xbf.toByte() -> Charsets.UTF_8 to 3
            bytes.size >= 2 && bytes[0] == 0xff.toByte() && bytes[1] == 0xfe.toByte() -> Charsets.UTF_16LE to 2
            bytes.size >= 2 && bytes[0] == 0xfe.toByte() && bytes[1] == 0xff.toByte() -> Charsets.UTF_16BE to 2
            else -> Charsets.UTF_8 to 0
        }
        val text = try {
            strictDecode(charset, bytes, skip)
        } catch (error: java.nio.charset.CharacterCodingException) {
            throw IllegalArgumentException("无法可靠识别编码，请另存为 UTF-8 或带 BOM 的 UTF-16", error)
        }
        require(text.length <= maxChars) { "解析内容超过 200 万字符限制" }
        require(text.none { it == '\u0000' || (it < ' ' && it !in "\t\r\n") }) { "文件包含二进制内容或不支持的文本编码" }
        return text
    }

    private fun strictDecode(charset: Charset, bytes: ByteArray, skip: Int) = charset.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes, skip, bytes.size - skip)).toString()

    fun text(bytes: ByteArray) = ParsedAttachment("character", listOf(decode(bytes))).validate()

    /** RFC 4180 quoting, including escaped quotes and embedded CR/LF. Retains original record text. */
    fun csv(bytes: ByteArray): ParsedAttachment {
        val source = decode(bytes)
        val records = mutableListOf<String>()
        var start = 0
        var index = 0
        var inQuotes = false
        var afterQuote = false
        var fieldStart = true
        fun record(end: Int) {
            require(end - start <= FileAttachmentLimits.MAX_UNIT_CHARS) { "CSV 单条记录超过 65536 字符限制" }
            records.add(source.substring(start, end))
        }
        while (index < source.length) {
            val c = source[index]
            if (inQuotes) {
                if (c == '"') {
                    if (source.getOrNull(index + 1) == '"') index++
                    else { inQuotes = false; afterQuote = true }
                }
            } else when (c) {
                '"' -> { require(fieldStart && !afterQuote) { "CSV 引号位置无效" }; inQuotes = true; fieldStart = false }
                ',' -> { fieldStart = true; afterQuote = false }
                '\r', '\n' -> {
                    record(index)
                    if (c == '\r' && source.getOrNull(index + 1) == '\n') index++
                    start = index + 1; fieldStart = true; afterQuote = false
                }
                else -> { require(!afterQuote) { "CSV 引号结束后只能是分隔符或换行" }; fieldStart = false }
            }
            index++
        }
        require(!inQuotes) { "CSV 引号未闭合" }
        if (start < source.length) record(source.length)
        return ParsedAttachment("record", records).validate()
    }
}
