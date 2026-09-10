package dev.clawseed.demo.sharing

import android.content.ClipData
import android.content.Intent
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import dev.clawseed.demo.MainActivity
import dev.clawseed.demo.ui.chat.ChatViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test

class ShareToChatDeviceTest {
    @Test fun createsNewDraftSessionsForColdAndWarmSharesWithoutSending() = runBlocking {
        SharedInboxDeviceTest.grantFixtures()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val image = SharedInboxDeviceTest.fixture("image")
        val md = SharedInboxDeviceTest.fixture("md")
        val first = Intent(context, MainActivity::class.java).setAction(Intent.ACTION_SEND_MULTIPLE).setType("*/*")
            .putParcelableArrayListExtra(Intent.EXTRA_STREAM, arrayListOf(image, md))
            .putExtra(Intent.EXTRA_TEXT, "SHARE-UI-81")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
        first.clipData = ClipData.newRawUri("fixtures", image).apply { addItem(ClipData.Item(md)) }
        ActivityScenario.launch<MainActivity>(first).use { scenario ->
            lateinit var vm: ChatViewModel
            scenario.onActivity { vm = ViewModelProvider(it)[ChatViewModel::class.java] }
            withTimeout(60_000) {
                while (vm.uiState.value.currentSessionId == null || vm.drafts.value[vm.uiState.value.currentSessionId] != "SHARE-UI-81") delay(100)
            }
            val firstId = checkNotNull(vm.uiState.value.currentSessionId)
            withTimeout(30_000) {
                while (SharedInbox(context).load(firstId)?.imported != true) delay(100)
            }
            val target = checkNotNull(vm.imageDraftTarget())
            assertEquals(1, vm.imageDrafts.value[target.key].orEmpty().size)
            assertEquals(1, vm.fileDrafts.value[target.key].orEmpty().size)
            assertTrue(vm.uiState.value.messages.isEmpty())
            assertFalse(vm.uiState.value.isGenerating)
            scenario.recreate()
            scenario.onActivity { vm = ViewModelProvider(it)[ChatViewModel::class.java] }
            delay(1000)
            assertEquals(firstId, vm.uiState.value.currentSessionId)
            assertEquals(1, vm.imageDrafts.value[target.key].orEmpty().size)
            assertEquals(1, vm.fileDrafts.value[target.key].orEmpty().size)

            val csv = SharedInboxDeviceTest.fixture("csv")
            val second = Intent(context, MainActivity::class.java).setAction(Intent.ACTION_SEND).setType("text/csv")
                .putExtra(Intent.EXTRA_STREAM, csv).putExtra(Intent.EXTRA_TEXT, "SHARE-UI-82")
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
            second.clipData = ClipData.newRawUri("fixture", csv)
            context.startActivity(second)
            withTimeout(60_000) {
                while (vm.uiState.value.currentSessionId == firstId || vm.drafts.value[vm.uiState.value.currentSessionId] != "SHARE-UI-82") delay(100)
            }
            val secondId = checkNotNull(vm.uiState.value.currentSessionId)
            assertNotEquals(firstId, secondId)
            assertEquals("SHARE-UI-81", vm.drafts.value[firstId])
            assertEquals(1, vm.imageDrafts.value[target.key].orEmpty().size)
            assertEquals(1, vm.fileDrafts.value[target.key].orEmpty().size)
            assertTrue(vm.uiState.value.messages.isEmpty())
            assertFalse(vm.uiState.value.isGenerating)
            println("Share UI sessions: first=$firstId second=$secondId; old draft preserved; no messages sent")
        }
    }
}
