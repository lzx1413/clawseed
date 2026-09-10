package dev.clawseed.demo.sharing

import android.content.ClipData
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.demo.ui.chat.ChatFileAttachments
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.Before
import java.io.File
import java.util.UUID

class SharedInboxDeviceTest {
    @Before fun grantFixtureAccess() { grantFixtures() }
    @Test fun multiShareCopiesMixedItemsDeduplicatesAndSurvivesReplay() = runBlocking {
        val target = InstrumentationRegistry.getInstrumentation().targetContext
        val directory = File(target.cacheDir, "share-test-${System.nanoTime()}").apply { mkdirs() }
        val context = object : ContextWrapper(target) { override fun getFilesDir() = directory }
        try {
            val inbox = SharedInbox(context)
            val id = UUID.randomUUID().toString()
            val image = fixture("image")
            val csv = fixture("csv")
            val intent = Intent(Intent.ACTION_SEND_MULTIPLE).setType("*/*")
                .putParcelableArrayListExtra(Intent.EXTRA_STREAM, arrayListOf(image, csv))
                .putExtra(Intent.EXTRA_TEXT, "分享问题 SHARE-71")
            intent.clipData = ClipData.newRawUri("duplicate", image).apply { addItem(ClipData.Item(csv)) }
            val bundle = inbox.receive(id, intent)
            assertEquals(2, bundle.items.size)
            assertEquals(1, bundle.items.count { it.image })
            assertEquals("分享问题 SHARE-71", bundle.text)
            assertEquals("分享 表格.csv", bundle.items[1].name)
            assertTrue(inbox.file(bundle, bundle.items[1]).readText().contains("SHARE-73\nsecond line"))
            assertEquals(bundle, SharedInbox(context).receive(id, Intent(Intent.ACTION_SEND)))
            inbox.bind(id, "gateway-a/session")
            assertTrue(runCatching { inbox.bind(id, "gateway-b/session") }.isFailure)
            val files = ChatFileAttachments(context).also { it.load() }
            val item = bundle.items[1]
            val attachmentId = "file_" + item.id.replace("-", "")
            repeat(2) { files.import("scope", Uri.fromFile(inbox.file(bundle, item)), attachmentId) }
            assertEquals(1, files.entries.value.getValue("scope").size)
            inbox.complete(id)
            assertTrue(checkNotNull(inbox.load(id)).imported)
            assertFalse(inbox.file(bundle, item).exists())
            assertTrue(files.metadata("scope").single().excerpt.content.contains("SHARE-73"))
            assertTrue(inbox.receive(id, intent).imported)
        } finally { directory.deleteRecursively() }
    }

    @Test fun rejectsPrivateUrisUnsupportedAndOversizedFilesWithoutLosingValidOnes() = runBlocking {
        val target = InstrumentationRegistry.getInstrumentation().targetContext
        val directory = File(target.cacheDir, "share-errors-${System.nanoTime()}").apply { mkdirs() }
        val context = object : ContextWrapper(target) { override fun getFilesDir() = directory }
        try {
            val inbox = SharedInbox(context)
            val intent = Intent(Intent.ACTION_SEND_MULTIPLE).setType("*/*").putParcelableArrayListExtra(Intent.EXTRA_STREAM, arrayListOf(
                fixture("md"), fixture("unsupported"), fixture("oversized"), fixture("denied"),
                Uri.parse("file:///data/user/0/dev.clawseed.demo/private.txt"),
                Uri.parse("content://dev.clawseed.demo.fileprovider/private.txt"),
            ))
            val bundle = inbox.receive(UUID.randomUUID().toString(), intent)
            assertEquals(1, bundle.items.size)
            assertEquals(5, bundle.warnings.size)
            assertTrue(inbox.file(bundle, bundle.items.single()).readText().contains("SHARE-72"))
            val tooMany = Intent(Intent.ACTION_SEND_MULTIPLE).putParcelableArrayListExtra(Intent.EXTRA_STREAM, ArrayList((1..9).map { fixture("md$it") }))
            assertTrue(runCatching { inbox.receive(UUID.randomUUID().toString(), tooMany) }.isFailure)
        } finally { directory.deleteRecursively() }
    }

    companion object {
        fun grantFixtures() {
            val descriptor = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(
                "content call --uri content://dev.clawseed.demo.test.share-fixtures --method grant",
            )
            android.os.ParcelFileDescriptor.AutoCloseInputStream(descriptor).use { it.readBytes() }
        }
        fun fixture(name: String): Uri = Uri.parse("content://dev.clawseed.demo.test.share-fixtures/$name")
    }
}
