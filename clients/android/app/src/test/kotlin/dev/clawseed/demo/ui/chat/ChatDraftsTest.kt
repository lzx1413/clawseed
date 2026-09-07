package dev.clawseed.demo.ui.chat

import androidx.lifecycle.SavedStateHandle
import org.junit.Assert.*
import org.junit.Test

class ChatDraftsTest {
    @Test fun draftsAreIndependentAndSurviveOwnerRecreation() {
        val handle = SavedStateHandle()
        val original = ChatDrafts(handle)
        original.update("a", "unfinished question")
        original.update("b", "another question")
        val restored = ChatDrafts(SavedStateHandle(mapOf("chat_drafts" to HashMap(original.drafts.value))))
        assertEquals("unfinished question", restored.drafts.value["a"])
        restored.update("b", "")
        assertEquals(mapOf("a" to "unfinished question"), restored.drafts.value)
    }

    @Test fun serverAssignedIdReceivesNewConversationDraft() {
        val drafts = ChatDrafts(SavedStateHandle())
        drafts.update("__new__", "draft before connection")
        drafts.moveNewDraft("assigned-id")
        assertEquals(mapOf("assigned-id" to "draft before connection"), drafts.drafts.value)
    }
}
