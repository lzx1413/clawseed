package dev.clawseed.demo.ui.chat

import androidx.lifecycle.SavedStateHandle

internal class ChatDrafts(private val state: SavedStateHandle) {
    val drafts = state.getStateFlow("chat_drafts", hashMapOf<String, String>())

    fun update(key: String, text: String) {
        val updated = HashMap(drafts.value)
        if (text.isEmpty()) updated.remove(key) else updated[key] = text
        state["chat_drafts"] = updated
    }

    fun moveNewDraft(sessionId: String) {
        val updated = HashMap(drafts.value)
        val draft = updated.remove("__new__") ?: return
        updated.putIfAbsent(sessionId, draft)
        state["chat_drafts"] = updated
    }
}
