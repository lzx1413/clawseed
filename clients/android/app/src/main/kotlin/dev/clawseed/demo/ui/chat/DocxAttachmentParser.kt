package dev.clawseed.demo.ui.chat

import dev.clawseed.sdk.core.model.FileAttachmentLimits
import org.xml.sax.Attributes
import org.xml.sax.InputSource
import org.xml.sax.helpers.DefaultHandler
import java.io.File
import java.io.StringReader
import java.util.zip.ZipFile
import javax.xml.parsers.SAXParserFactory

internal object DocxAttachmentParser {
    private const val WORD_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    private const val STRICT_WORD_NS = "http://purl.oclc.org/ooxml/wordprocessingml/main"

    fun parse(file: File): ParsedAttachment = ZipFile(file).use { zip ->
        require(zip.size() <= 2_000) { "DOCX 内部文件数量超过限制" }
        val entry = zip.getEntry("word/document.xml") ?: error("不是有效的 DOCX 文件；旧 DOC 请另存为 DOCX")
        val bytes = zip.getInputStream(entry).use { input ->
            input.readBytesBounded(8 * 1024 * 1024)
        }
        val xml = TextAttachmentParser.decode(bytes, 8 * 1024 * 1024)
        require(!xml.contains("<!DOCTYPE", ignoreCase = true) && !xml.contains("<!ENTITY", ignoreCase = true)) { "DOCX 不允许包含 DTD 或实体声明" }
        val handler = WordHandler()
        val reader = SAXParserFactory.newInstance().apply { isNamespaceAware = true }.newSAXParser().xmlReader
        reader.contentHandler = handler
        reader.setEntityResolver { _, _ -> throw IllegalArgumentException("DOCX 不允许外部实体") }
        reader.parse(InputSource(StringReader(xml)))
        require(handler.sawDocument) { "DOCX 主文档格式无效" }
        ParsedAttachment("character", listOf(handler.output.toString().trim())).validate()
    }

    private class WordHandler : DefaultHandler() {
        val output = StringBuilder()
        var sawDocument = false
        private val paragraph = StringBuilder()
        private var text = false
        private var prefix = ""
        private var inCell = false
        override fun startElement(uri: String, local: String, qName: String, attributes: Attributes) {
            if (uri != WORD_NS && uri != STRICT_WORD_NS) return
            when (local) {
                "document" -> sawDocument = true
                "p" -> { paragraph.clear(); prefix = "" }
                "t" -> text = true
                "tab" -> paragraph.append('\t')
                "br", "cr" -> paragraph.append('\n')
                "numPr" -> prefix = "- "
                "pStyle" -> {
                    val value = attributes.getValue(uri, "val").orEmpty()
                    val level = Regex("(?i)heading([1-6])").matchEntire(value)?.groupValues?.get(1)?.toInt()
                    if (level != null) prefix = "#".repeat(level) + " "
                }
                "tc" -> inCell = true
            }
        }
        override fun characters(ch: CharArray, start: Int, length: Int) {
            if (text) {
                require(output.length.toLong() + paragraph.length + length <= FileAttachmentLimits.MAX_PARSED_CHARS) { "DOCX 文字超过 200 万字符限制" }
                paragraph.append(ch, start, length)
            }
        }
        override fun endElement(uri: String, local: String, qName: String) {
            if (uri != WORD_NS && uri != STRICT_WORD_NS) return
            when (local) {
                "t" -> text = false
                "p" -> { output.append(prefix).append(paragraph).append(if (inCell) " / " else "\n\n"); paragraph.clear() }
                "tc" -> { if (output.endsWith(" / ")) output.setLength(output.length - 3); output.append('\t'); inCell = false }
                "tr" -> { if (output.endsWith("\t")) output.setLength(output.length - 1); output.append('\n') }
            }
            require(output.length <= FileAttachmentLimits.MAX_PARSED_CHARS) { "DOCX 文字超过 200 万字符限制" }
        }
    }
}

internal fun java.io.InputStream.readBytesBounded(limit: Int): ByteArray {
    val output = java.io.ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val read = read(buffer)
        if (read < 0) break
        require(output.size().toLong() + read <= limit) { "文件内容超过读取限制" }
        output.write(buffer, 0, read)
    }
    return output.toByteArray()
}
