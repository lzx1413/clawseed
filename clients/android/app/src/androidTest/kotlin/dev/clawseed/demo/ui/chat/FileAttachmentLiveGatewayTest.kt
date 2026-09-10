package dev.clawseed.demo.ui.chat

import androidx.test.platform.app.InstrumentationRegistry
import com.tom_roush.pdfbox.android.PDFBoxResourceLoader
import com.tom_roush.pdfbox.pdmodel.PDDocument
import com.tom_roush.pdfbox.pdmodel.PDPage
import com.tom_roush.pdfbox.pdmodel.PDPageContentStream
import com.tom_roush.pdfbox.pdmodel.font.PDType1Font
import dev.clawseed.sdk.core.ClawSeed
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.FileAttachment
import dev.clawseed.sdk.core.model.AttachmentReadResult
import dev.clawseed.sdk.core.tool.ToolResult
import dev.clawseed.sdk.embedded.EmbeddedGateway
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onEach
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

/** Explicitly invoked acceptance test using the installed release and configured live model. */
class FileAttachmentLiveGatewayTest {
    @Test fun formatsFilesOnlyLongContinuationAndMixedImage() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val sources = File(context.cacheDir, "file-formats-${System.nanoTime()}").apply { mkdirs() }
        val store = ChatFileAttachments(context).also { it.load() }
        val gateway = EmbeddedGateway(context)
        gateway.start()
        val session = ClawSeed.createSession(gateway.localConfig())
        val ranges = java.util.Collections.synchronizedList(mutableListOf<Pair<Int, Int>>())
        session.tools.register("attachment_read", "Read the attached file by zero-based start and count. Continue with next until eof.",
            """{"type":"object","properties":{"attachment_id":{"type":"string"},"start":{"type":"integer"},"count":{"type":"integer"}},"required":["attachment_id","start","count"]}""") { args ->
            val sid = session.sessionInfo.value!!.sessionId
            val result = store.read(ChatImageDrafts.key(session.gateway, sid), args.getValue("attachment_id").jsonPrimitive.content,
                args.getValue("start").jsonPrimitive.int, args.getValue("count").jsonPrimitive.int)
            ranges.add(result.start to result.next)
            ToolResult.Success(Json.encodeToString(AttachmentReadResult.serializer(), result))
        }
        try {
            withTimeout(30_000) { session.connect(); session.sessionInfo.first { it != null } }
            val info = session.sessionInfo.value!!
            val key = ChatImageDrafts.key(session.gateway, info.sessionId)
            val md = File(sources, "中文 说明.md").apply { writeText("# 附件验收\nMarkdown 校验码：CEDAR-31。\n请列出本次附件里的全部校验码。") }
            val csv = File(sources, "引号与换行.csv").apply { writeText("名称,说明\r\n\"青,禾\",\"第一行\r\n校验码 CLOVER-91，含\"\"引号\"\"\"\r\n") }
            val docx = File(sources, "Word 表格.docx")
            java.util.zip.ZipOutputStream(docx.outputStream()).use { zip ->
                zip.putNextEntry(java.util.zip.ZipEntry("word/document.xml"))
                zip.write("""<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>验收表格</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>校验码</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>MAPLE-72</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>""".toByteArray())
                zip.closeEntry()
            }
            for (source in listOf(md, csv, docx)) store.import(key, android.net.Uri.fromFile(source))
            val files = store.metadata(key)
            assertEquals(3, files.size)
            assertTrue(files.single { it.format == "csv" }.excerpt.content.contains("第一行\r\n校验码"))
            store.markSending(key, files.map { it.id }.toSet())
            val answer = turn(session, "", files)
            for (code in listOf("CEDAR-31", "CLOVER-91", "MAPLE-72")) assertTrue(answer, answer.contains(code))
            store.finish(key, files.map { it.id }.toSet(), true)
            val long = File(sources, "长文档.txt").apply { writeText("这是顺序读取测试，请读到结尾。\n" + "无校验码的填充内容。\n".repeat(1900) + "全文唯一校验码：SEQUOIA-83。") }
            store.import(key, android.net.Uri.fromFile(long))
            val longFiles = store.metadata(key)
            assertFalse(longFiles.single().excerpt.eof)
            assertFalse(longFiles.single().excerpt.content.contains("SEQUOIA-83"))
            store.markSending(key, longFiles.map { it.id }.toSet())
            ranges.clear()
            val full = turn(session, "请读取本次长 TXT 全文。从预览的 next 开始，每次 attachment_read count=8000，按返回 next 顺序续读直到 eof=true，再只回答全文唯一校验码。", longFiles)
            assertTrue(full, full.contains("SEQUOIA-83"))
            assertTrue("Expected multiple reads: $ranges", ranges.size >= 2)
            assertTrue("Expected continuation offsets: $ranges", ranges.zipWithNext().any { (a, b) -> a.second == b.first })
            store.finish(key, longFiles.map { it.id }.toSet(), true)
            InstrumentationRegistry.getInstrumentation().sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\nFiles-only MD/CSV/DOCX passed; long TXT ranges=$ranges; model=${session.gateway.status().getOrThrow().model}\n")
            })
            assertTrue("Configured model must support mixed image acceptance", info.imageAttachmentsSupported)
            val bitmap = android.graphics.Bitmap.createBitmap(800, 600, android.graphics.Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.RED) }
            val png = java.io.ByteArrayOutputStream().use { out -> bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out); out.toByteArray() }
            bitmap.recycle()
            val image = session.gateway.uploadImage(info.sessionId, png).getOrThrow()
            val imageOnly = turn(session, "请只回答这张图片的主要颜色。", emptyList(), listOf(image))
            assertTrue("Image-only: $imageOnly", imageOnly.contains("红") || imageOnly.contains("red", ignoreCase = true))
            val small = android.graphics.Bitmap.createBitmap(80, 80, android.graphics.Bitmap.Config.ARGB_8888).apply { eraseColor(android.graphics.Color.BLUE) }
            val smallPng = java.io.ByteArrayOutputStream().use { out -> small.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, out); out.toByteArray() }
            small.recycle()
            val mixedImage = session.gateway.uploadImage(info.sessionId, smallPng).getOrThrow()
            store.import(key, android.net.Uri.fromFile(md))
            val mixedFiles = store.metadata(key)
            store.markSending(key, mixedFiles.map { it.id }.toSet())
            val mixed = turn(session, "请回答本次 Markdown 的校验码，以及本轮新上传图片的主要颜色（与上一轮不同）。", mixedFiles, listOf(mixedImage))
            assertTrue(mixed, mixed.contains("CEDAR-31"))
            assertTrue(mixed, mixed.contains("蓝") || mixed.contains("blue", ignoreCase = true))
            store.finish(key, mixedFiles.map { it.id }.toSet(), true)
            val history = session.gateway.sessionMessages(info.sessionId).getOrThrow()
            assertEquals(3, history.first { it.role == "user" }.files.size)
            val last = history.last { it.role == "user" }
            assertEquals(1, last.files.size); assertEquals(1, last.attachments.size)
            InstrumentationRegistry.getInstrumentation().sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\nMD/CSV/DOCX files-only answers verified; CSV multiline preserved; long TXT continuation $ranges; mixed image+file+text verified. Session: ${info.sessionId}\n")
            })
        } finally { session.close(); gateway.stop(); sources.deleteRecursively() }
    }

    @Test fun pdfSendAndContinuationThroughRealGateway() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val source = File(context.cacheDir, "附件验收 文本.pdf")
        PDFBoxResourceLoader.init(context)
        PDDocument().use { document ->
            repeat(2) { index ->
                val page = PDPage(); document.addPage(page)
                PDPageContentStream(document, page).use { stream ->
                    stream.beginText(); stream.setFont(PDType1Font.HELVETICA, 12f); stream.newLineAtOffset(30f, 700f)
                    stream.showText("Verification code on page ${index + 1}: ORCHID-${index + 41}"); stream.endText()
                }
            }
            document.save(source)
        }
        val store = ChatFileAttachments(context)
        store.load()
        val gateway = EmbeddedGateway(context)
        gateway.start()
        val config = gateway.localConfig()
        val calls = AtomicInteger()
        fun makeSession() = ClawSeed.createSession(config).also { session ->
            session.tools.register("attachment_read", "Read attached documents by attachment ID and zero-based range.",
                """{"type":"object","properties":{"attachment_id":{"type":"string"},"start":{"type":"integer"},"count":{"type":"integer"}},"required":["attachment_id","start","count"]}""") { args ->
                calls.incrementAndGet()
                val sid = checkNotNull(session.sessionInfo.value).sessionId
                val output = store.read(ChatImageDrafts.key(session.gateway, sid), args.getValue("attachment_id").jsonPrimitive.content,
                    args.getValue("start").jsonPrimitive.int, args.getValue("count").jsonPrimitive.int)
                ToolResult.Success(Json.encodeToString(AttachmentReadResult.serializer(), output))
            }
        }
        var session = makeSession()
        try {
            withTimeout(30_000) { session.connect(); session.sessionInfo.first { it != null } }
            val info = session.sessionInfo.value!!
            assertTrue("Installed gateway must advertise file attachments", info.fileAttachmentsSupported)
            val key = ChatImageDrafts.key(session.gateway, info.sessionId)
            store.import(key, android.net.Uri.fromFile(source))
            val files = store.metadata(key)
            assertEquals("ready", files.single().status)
            store.markSending(key, files.map { it.id }.toSet())
            val result = turn(session, "这是附件功能验收。请只回答 PDF 第 2 页的 Verification code。", files)
            assertTrue(result, result.contains("ORCHID-42"))
            store.finish(key, files.map { it.id }.toSet(), true)
            val history = session.gateway.sessionMessages(info.sessionId).getOrThrow()
            assertEquals(files.single().id, history.first { it.role == "user" }.files.single().id)
            session.close()
            session = makeSession()
            withTimeout(30_000) { session.connect(info.sessionId); session.sessionInfo.first { it != null } }
            val reread = turn(session, "请必须调用 attachment_read 重新读取刚才的 PDF 第 1 页（start=0,count=1），只回答该页 Verification code。", emptyList())
            assertTrue(reread, reread.contains("ORCHID-41"))
            assertTrue("Reconnected client must execute the registered attachment tool", calls.get() > 0)
            InstrumentationRegistry.getInstrumentation().sendStatus(0, android.os.Bundle().apply {
                putString("stream", "\nLive PDF accepted; answer ORCHID-42; history restored; reconnect executed attachment_read and answered ORCHID-41. Session: ${info.sessionId}\n")
            })
        } finally { session.close(); gateway.stop(); source.delete() }
    }

    private suspend fun turn(session: ClawSeedSession, text: String, files: List<FileAttachment>, images: List<dev.clawseed.sdk.core.model.ImageAttachment> = emptyList()): String = coroutineScope {
        var imageCount = -1
        val result = async(start = CoroutineStart.UNDISPATCHED) {
            withTimeout(120_000) { session.events.onEach { event ->
                if (event is ChatEvent.DebugPrompt && images.isNotEmpty()) {
                    val last = Json.parseToJsonElement(event.messages).jsonArray.last { it.jsonObject["role"]?.jsonPrimitive?.content == "user" }.jsonObject
                    imageCount = last["attachments"]?.jsonArray?.size ?: 0
                }
            }.first { it is ChatEvent.Done || it is ChatEvent.Error } }
        }
        session.sendMessage(text, true, images, files)
        when (val event = result.await()) {
            is ChatEvent.Done -> {
                if (images.isNotEmpty()) assertEquals("Images in Agent provider-input debug payload", images.size, imageCount)
                event.fullResponse
            }
            is ChatEvent.Error -> error("Gateway rejected attachment turn: ${event.message}")
            else -> error("Unexpected completion")
        }
    }
}
