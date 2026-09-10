package dev.clawseed.demo.ui.chat

import android.content.Context
import com.tom_roush.pdfbox.android.PDFBoxResourceLoader
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.text.PDFTextStripper
import dev.clawseed.sdk.core.model.FileAttachmentLimits
import java.io.File
import java.io.Writer

internal object PdfAttachmentParser {
    fun parse(context: Context, file: File): ParsedAttachment {
        require(file.inputStream().use { it.readBytesBoundedHeader(5) }.contentEquals("%PDF-".toByteArray())) { "PDF 文件头无效" }
        PDFBoxResourceLoader.init(context.applicationContext)
        try {
            return PDDocument.load(file).use { document ->
                require(!document.isEncrypted) { "暂不支持加密 PDF，请先解密" }
                require(document.numberOfPages in 1..FileAttachmentLimits.MAX_PAGES) { "PDF 页数须为 1..300" }
                var total = 0
                val pages = (1..document.numberOfPages).map { page ->
                    val output = StringBuilder()
                    val writer = object : Writer() {
                        override fun write(chars: CharArray, offset: Int, length: Int) {
                            require(output.length.toLong() + length <= FileAttachmentLimits.MAX_UNIT_CHARS) { "PDF 单页文字超过 65536 字符限制" }
                            require(total.toLong() + length <= FileAttachmentLimits.MAX_PARSED_CHARS) { "PDF 文字超过 200 万字符限制" }
                            output.append(chars, offset, length)
                            total += length
                        }
                        override fun flush() = Unit
                        override fun close() = Unit
                    }
                    PDFTextStripper().apply { startPage = page; endPage = page; sortByPosition = true }.writeText(document, writer)
                    output.toString()
                }
                require(pages.any { it.isNotBlank() }) { "此 PDF 没有可提取文字，扫描件需要 OCR" }
                ParsedAttachment("page", pages).validate()
            }
        } catch (error: com.tom_roush.pdfbox.pdmodel.encryption.InvalidPasswordException) {
            throw IllegalArgumentException("暂不支持加密 PDF，请先解密", error)
        } catch (error: java.io.IOException) {
            throw IllegalArgumentException("PDF 已损坏或无法解析：${error.message}", error)
        }
    }
}

private fun java.io.InputStream.readBytesBoundedHeader(count: Int): ByteArray {
    val bytes = ByteArray(count)
    var offset = 0
    while (offset < count) {
        val n = read(bytes, offset, count - offset)
        if (n < 0) break
        offset += n
    }
    return bytes.copyOf(offset)
}
