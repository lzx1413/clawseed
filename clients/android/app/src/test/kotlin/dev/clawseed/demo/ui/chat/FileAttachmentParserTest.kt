package dev.clawseed.demo.ui.chat

import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class FileAttachmentParserTest {
    @Test fun unicodeAndEncodingBoundaries() {
        val parsed = TextAttachmentParser.text("甲😀乙".toByteArray())
        assertEquals(3, parsed.total)
        assertEquals("😀", parsed.read("a", 1, 1).content)
        assertEquals("乙", parsed.read("a", 2, 100).content)
        assertTrue(parsed.read("a", 3).eof)
        assertEquals("中文", TextAttachmentParser.decode(byteArrayOf(0xff.toByte(), 0xfe.toByte()) + "中文".toByteArray(Charsets.UTF_16LE)))
        assertThrows(IllegalArgumentException::class.java) { TextAttachmentParser.text(byteArrayOf(0xff.toByte())) }
        assertThrows(IllegalArgumentException::class.java) { TextAttachmentParser.text(byteArrayOf(0)) }
    }

    @Test fun csvPreservesQuotedMultilineRecordsAndContinuation() {
        val parsed = TextAttachmentParser.csv("名称,说明\r\n甲,\"有逗号,有\r\n换行和\"\"引号\"\"\"\r\n乙,结束\r\n".toByteArray())
        assertEquals(3, parsed.total)
        val result = parsed.read("a", 1, 1)
        assertTrue(result.content.contains("有\r\n换行"))
        assertEquals(2, result.next)
        assertFalse(result.eof)
        assertTrue(parsed.read("a", result.next, 1).eof)
        assertEquals("名称,说明", parsed.parts.first())
        assertThrows(IllegalArgumentException::class.java) { TextAttachmentParser.csv("a,\"open".toByteArray()) }
        assertThrows(IllegalArgumentException::class.java) { TextAttachmentParser.csv("a,\"closed\"junk".toByteArray()) }
    }

    @Test fun wholeRecordBudgetNeverSilentlySplitsCsv() {
        val parsed = TextAttachmentParser.csv("head\n\"${"x".repeat(2000)}\nrest\"".toByteArray())
        val preview = parsed.read("a", budget = 1000, allowLargeUnit = false)
        assertEquals(1, preview.next)
        val next = parsed.read("a", preview.next, budget = 1000)
        assertEquals(2, next.next)
        assertTrue(next.content.contains("\nrest\""))
        assertTrue(next.eof)
    }

    @Test fun docxExtractsHeadingsListsAndTablesAndRejectsEntities() {
        val body = """<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
          <w:p><w:pPr><w:pStyle w:val="Heading2"/></w:pPr><w:r><w:t>标题</w:t></w:r></w:p>
          <w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>条目</w:t></w:r></w:p>
          <w:tbl><w:tr><w:tc><w:p><w:r><w:t>名称</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>数值</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
        </w:body></w:document>"""
        withDocx(body) { file ->
            val text = DocxAttachmentParser.parse(file).parts.single()
            assertTrue(text.contains("## 标题"))
            assertTrue(text.contains("- 条目"))
            assertTrue(text.contains("名称\t数值"))
        }
        withDocx("<!DOCTYPE document [<!ENTITY x SYSTEM 'file:///etc/passwd'>]>$body") { file ->
            assertThrows(IllegalArgumentException::class.java) { DocxAttachmentParser.parse(file) }
        }
    }

    private fun withDocx(xml: String, block: (File) -> Unit) {
        val file = File.createTempFile("attachment", ".docx")
        try {
            ZipOutputStream(file.outputStream()).use { zip ->
                zip.putNextEntry(ZipEntry("word/document.xml")); zip.write(xml.toByteArray()); zip.closeEntry()
            }
            block(file)
        } finally { file.delete() }
    }
}
