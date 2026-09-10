package dev.clawseed.demo.ui.chat

import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.sdk.core.ClawSeed
import dev.clawseed.sdk.embedded.EmbeddedGateway
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.flow.first
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

/** Run prepare and verify in separate instrumentation processes, with an App restart between them. */
class FileAttachmentRestartTest {
    @Test fun prepareOrVerifyDraftAcrossProcessRestart() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val record = File(context.cacheDir, "attachment-restart-case.json")
        val store = ChatFileAttachments(context).also { it.load() }
        if (InstrumentationRegistry.getArguments().getString("phase") == "prepare") {
            val gateway = EmbeddedGateway(context)
            gateway.start()
            val session = ClawSeed.createSession(gateway.localConfig())
            try {
                withTimeout(30_000) { session.connect(); session.sessionInfo.first { it != null } }
                val id = session.sessionInfo.value!!.sessionId
                val key = ChatImageDrafts.key(session.gateway, id)
                val source = File(context.cacheDir, "重启恢复 附件.csv").apply { writeText("项目,内容\n测试,\"跨行\nRESTART-47\"\n") }
                store.import(key, android.net.Uri.fromFile(source))
                source.delete()
                val file = store.metadata(key).single()
                store.markSending(key, setOf(file.id))
                store.finish(key, setOf(file.id), false)
                val textStore = ChatImageDrafts(context).also { it.load() }
                textStore.saveText(key, "Report the verification code in the attached CSV.")
                record.writeText(JSONObject().put("session", id).put("key", key).put("attachment", file.id).toString())
            } finally { session.close(); gateway.stop() }
        } else {
            assertTrue("Prepare must run in an earlier process", record.exists())
            val saved = JSONObject(record.readText())
            val key = saved.getString("key")
            val id = saved.getString("attachment")
            assertEquals(id, store.metadata(key).single().id)
            assertFalse(store.entries.value.getValue(key).single().awaitingReply)
            assertFalse(store.entries.value.getValue(key).single().sent)
            assertTrue(store.read(key, id, 1, 1).content.contains("跨行\nRESTART-47"))
            val textStore = ChatImageDrafts(context).also { it.load() }
            assertEquals("Report the verification code in the attached CSV.", textStore.savedText(key))
            try { store.read("other-session", id, 0, 1); fail("Cross-session access allowed") } catch (expected: IllegalStateException) { }
        }
        val saved = JSONObject(record.readText())
        InstrumentationRegistry.getInstrumentation().sendStatus(0, android.os.Bundle().apply {
            putString("stream", "\nRestart fixture: session=${saved.getString("session")}; retained original copy, CSV records, failed-send draft and question.\n")
        })
    }
}
