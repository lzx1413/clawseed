package dev.clawseed.demo.ui.chat.components

import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class DebugPromptMessagesTest {
    @Test fun distinguishesHistoricalMemoryFromCurrentInputWithoutChangingTheRequest() {
        val raw = """[
            {"role":"system","content":"fixed memory"},
            {"role":"user","content":"[Memory context]\nold memory\n[/Memory context]\nold question"},
            {"role":"assistant","content":"answer","tool_calls":[{"id":"tool-1"}]},
            {"role":"tool","content":"result"},
            {"role":"user","content":[{"type":"text","text":"new question"},{"type":"image_url","image_url":{"url":"image"}}]}
        ]"""
        val messages = debugPromptMessages(raw)!!
        assertEquals(listOf(DebugPromptScope.System, DebugPromptScope.History, DebugPromptScope.History, DebugPromptScope.History, DebugPromptScope.Current), messages.map { it.scope })
        assertEquals(Json.parseToJsonElement(raw), Json.parseToJsonElement(messages.joinToString(prefix = "[", postfix = "]") { it.json }))
    }

    @Test fun handlesMissingUserAndMalformedPayloadsWithoutInventingACurrentTurn() {
        assertEquals(DebugPromptScope.System, debugPromptMessages("""[{"role":"system","content":"prompt"}]""")!!.single().scope)
        assertNull(debugPromptMessages("invalid json"))
        assertNull(debugPromptMessages("{}"))
    }
}
