package dev.clawseed.demo.ui.chat

import android.content.Context
import android.content.ContextWrapper
import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Before
import org.junit.After
import org.junit.Test
import org.junit.Assert.*
import org.junit.runner.RunWith
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.tom_roush.pdfbox.android.PDFBoxResourceLoader
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.pdmodel.PDPage
import com.tom_roush.pdfbox.pdmodel.PDPageContentStream
import com.tom_roush.pdfbox.pdmodel.font.PDType1Font
import com.tom_roush.pdfbox.pdmodel.encryption.AccessPermission
import com.tom_roush.pdfbox.pdmodel.encryption.StandardProtectionPolicy
import kotlinx.coroutines.runBlocking
import java.io.File

/** Runs against the release APK with an isolated files directory, preserving real user data. */
@RunWith(AndroidJUnit4::class)
class FileAttachmentDeviceTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private lateinit var directory: File
    private lateinit var context: Context
    @Before fun setUp() {
        val target = instrumentation.targetContext
        directory = File(target.cacheDir, "attachment-tests-${System.nanoTime()}").apply { mkdirs() }
        context = object : ContextWrapper(target) { override fun getFilesDir() = directory }
    }
    @After fun tearDown() {
        directory.deleteRecursively()
    }

    @Test fun testPersistentScopeBudgetAndMissingFiles() = runBlocking {
        val source = File(directory, "中文 文件.md").apply { writeText("机密会话内容😀\n" + "x".repeat(20_000)) }
        var store = ChatFileAttachments(context)
        store.load()
        store.import("gateway-a/session-a", Uri.fromFile(source))
        val draft = store.entries.value.getValue("gateway-a/session-a").single()
        assertEquals("ready", draft.status)
        val metadata = store.metadata("gateway-a/session-a").single()
        assertFalse(metadata.excerpt.eof)
        assertTrue(metadata.excerpt.content.length <= 1000)
        expectFailure { store.read("gateway-a/session-a", draft.id, 0, 100) }
        store.markSending("gateway-a/session-a", setOf(draft.id))
        expectFailure { store.read("gateway-b/session-a", draft.id, 0, 100) }
        expectFailure { store.read("gateway-a/session-b", draft.id, 0, 100) }
        assertTrue(store.read("gateway-a/session-a", draft.id, 0, 100).content.contains("机密会话"))
        store.finish("gateway-a/session-a", setOf(draft.id), true)
        store.remove("gateway-a/session-a", draft.id)
        store = ChatFileAttachments(context)
        store.load()
        assertTrue(store.entries.value.getValue("gateway-a/session-a").single().sent)
        assertTrue(store.read("gateway-a/session-a", draft.id, 0, 100).content.contains("机密会话"))
        File(directory, "chat-file-attachments/${draft.id}.original").delete()
        expectFailure { store.read("gateway-a/session-a", draft.id, 0, 100) }
    }

    @Test fun testSameNamesAndCombinedExcerptBudget() = runBlocking {
        val source = File(directory, "重复.txt").apply { writeText("😀".repeat(4000)) }
        val store = ChatFileAttachments(context)
        store.load()
        repeat(4) { store.import("scope", Uri.fromFile(source)) }
        val files = store.metadata("scope")
        assertEquals(4, files.map { it.id }.toSet().size)
        assertTrue(files.sumOf { it.excerpt.content.length } <= 16_000)
        assertTrue(files.all { it.name == "重复.txt" })
        expectFailure { store.import("scope", Uri.fromFile(source)) }
        val id = files.first().id
        store.markSending("scope", setOf(id))
        store.finish("scope", setOf(id), false)
        assertFalse(store.entries.value.getValue("scope").first().sent)
        assertFalse(store.entries.value.getValue("scope").first().awaitingReply)
    }

    @Test fun testReleasePdfPagesEncryptionAndScan() {
        PDFBoxResourceLoader.init(instrumentation.targetContext)
        val file = File(directory, "pages.pdf")
        PDDocument().use { document ->
            repeat(2) { index ->
                val page = PDPage(); document.addPage(page)
                PDPageContentStream(document, page).use { stream ->
                    stream.beginText(); stream.setFont(PDType1Font.HELVETICA, 12f)
                    stream.newLineAtOffset(30f, 700f); stream.showText("Attachment page ${index + 1}: ORCHID-${index + 41}"); stream.endText()
                }
            }
            document.save(file)
        }
        val parsed = PdfAttachmentParser.parse(context, file)
        assertEquals(2, parsed.total)
        val result = parsed.read("pdf", 1, 1)
        assertTrue(result.content.contains("ORCHID-42"))
        assertTrue(result.eof)
        PDDocument().use { document -> document.addPage(PDPage()); document.save(file) }
        try { PdfAttachmentParser.parse(context, file); fail("Scan should require OCR") } catch (expected: IllegalArgumentException) { assertTrue(expected.message!!.contains("OCR")) }
        PDDocument().use { document ->
            document.addPage(PDPage())
            document.protect(StandardProtectionPolicy("owner", "user", AccessPermission()))
            document.save(file)
        }
        try { PdfAttachmentParser.parse(context, file); fail("Encrypted PDF accepted") } catch (expected: IllegalArgumentException) { assertTrue(expected.message!!.contains("加密")) }
    }

    @Test fun testInvalidImportsAndInterruptedRetry() = runBlocking {
        val store = ChatFileAttachments(context).also { it.load() }
        val badPdf = File(directory, "损坏.pdf").apply { writeText("%PDF-1.4\nbroken") }
        store.import("invalid-pdf", Uri.fromFile(badPdf))
        assertEquals("failed", store.entries.value.getValue("invalid-pdf").single().status)
        val binary = File(directory, "二进制.md").apply { writeBytes(byteArrayOf(0, 1, 2)) }
        store.import("invalid-text", Uri.fromFile(binary))
        assertEquals("failed", store.entries.value.getValue("invalid-text").single().status)
        val unsupported = File(directory, "旧文档.doc").apply { writeText("unsupported") }
        expectFailure { store.import("unsupported", Uri.fromFile(unsupported)) }
        val big = File(directory, "超限.txt")
        java.io.RandomAccessFile(big, "rw").use { it.setLength(21L * 1024 * 1024) }
        store.import("oversized", Uri.fromFile(big))
        assertTrue(store.entries.value.getValue("oversized").single().error!!.contains("20 MiB"))
        val good = File(directory, "恢复.md").apply { writeText("恢复后校验码 RETRY-52") }
        store.import("retry", Uri.fromFile(good))
        val manifest = File(directory, "chat-file-attachments/manifest.json")
        val json = org.json.JSONObject(manifest.readText())
        json.getJSONArray("retry").getJSONObject(0).put("status", "parsing")
        manifest.writeText(json.toString())
        val restored = ChatFileAttachments(context).also { it.load() }
        val draft = restored.entries.value.getValue("retry").single()
        assertEquals("failed", draft.status)
        assertTrue(draft.error!!.contains("中断"))
        restored.retry("retry", draft.id)
        assertEquals("ready", restored.entries.value.getValue("retry").single().status)
        assertTrue(restored.metadata("retry").single().excerpt.content.contains("RETRY-52"))
    }

    private suspend fun expectFailure(block: suspend () -> Unit) {
        try { block() } catch (expected: IllegalArgumentException) { return } catch (expected: IllegalStateException) { return }
        fail("Expected attachment error")
    }
}
