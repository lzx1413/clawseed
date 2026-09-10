package dev.clawseed.demo.ui.chat

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ImageDraftPersistenceTest {
    @Test fun writesAreDispatchedAndConcurrentUpdatesDoNotOverwriteEachOther() = runTest {
        var disk = ImageDraftSnapshot()
        var writes = 0
        val store = ImageDraftPersistence({ disk }, { disk = it; writes++ }, StandardTestDispatcher(testScheduler))
        val first = async { store.update { it.copy(texts = it.texts + ("first" to "one")) } }
        val second = async { store.update { it.copy(texts = it.texts + ("second" to "two")) } }
        assertEquals(0, writes)
        first.await()
        second.await()
        assertEquals(mapOf("first" to "one", "second" to "two"), disk.texts)
        assertEquals("one", store.savedText("first"))
    }

    @Test fun batchTransitionUsesOneWriteAndDraftsRecoverAfterRestart() = runTest {
        val images = (1..4).map { ChatImageDraft("image-$it") }
        var disk = ImageDraftSnapshot(mapOf("session" to images), mapOf("session" to "question"))
        var writes = 0
        fun newStore() = ImageDraftPersistence({ disk }, { disk = it; writes++ }, StandardTestDispatcher(testScheduler))
        val store = newStore()
        store.update { state ->
            state.copy(images = state.images + ("session" to images.map { it.copy(awaitingReply = true) }))
        }
        assertEquals(1, writes)
        assertTrue(store.drafts.value.getValue("session").all { it.awaitingReply })
        val restored = newStore()
        restored.load()
        assertEquals(images, restored.drafts.value.getValue("session"))
        assertEquals("question", restored.savedText("session"))
    }

    @Test fun interruptedPreparationRecoversAsRetryable() = runTest {
        val disk = ImageDraftSnapshot(mapOf("session" to listOf(ChatImageDraft("photo", preparing = true))))
        val store = ImageDraftPersistence({ disk }, {}, StandardTestDispatcher(testScheduler))
        store.load()
        val draft = store.drafts.value.getValue("session").single()
        assertFalse(draft.preparing)
        assertFalse(draft.uploading)
        assertEquals("图片处理中断，请重试", draft.error)
    }

    @Test fun failedWriteDoesNotPublishOrDiscardTheDurableDraft() = runTest {
        val disk = ImageDraftSnapshot(mapOf("session" to listOf(ChatImageDraft("keep"))))
        val store = ImageDraftPersistence({ disk }, { error("disk full") }, StandardTestDispatcher(testScheduler))
        store.load()
        val result = runCatching { store.update { ImageDraftSnapshot() } }
        assertTrue(result.isFailure)
        assertEquals(disk.images, store.drafts.value)
    }
}
