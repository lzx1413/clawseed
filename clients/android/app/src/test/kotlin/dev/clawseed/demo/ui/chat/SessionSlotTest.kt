package dev.clawseed.demo.ui.chat

import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.ToolCallInfo
import dev.clawseed.sdk.android.ChatAccumulator
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class SessionSlotTest {
    private val history = listOf(
        ChatEntry.UserMessage("hist-0", 0, "old question"),
        ChatEntry.ToolInvocations("tools-0", 0, listOf(ToolCallInfo("call", "search", "{}", "result", true))),
        ChatEntry.AssistantMessage("hist-1", 0, "old answer"),
    )

    @Test fun restoringSlotKeepsHistoryToolsAndBackgroundResponse() = runTest {
        val session = FakeSession()
        val acc = ChatAccumulator(session).also { it.startIn(backgroundScope) }
        runCurrent()
        val slots = mutableMapOf("old" to SessionSlot(session, acc, history))
        acc.addUserMessage("new question")
        session.events.emit(ChatEvent.Done("new answer"))
        runCurrent()
        val restored = slots.getValue("old").messages()
        assertEquals(history, restored.take(3))
        assertEquals("new answer", (restored.last() as ChatEntry.AssistantMessage).content)
        assertEquals(5, restored.size)
    }

    @Test fun regeneratingLoadedHistoryReplacesOnlyTheLastReply() = runTest {
        val session = FakeSession()
        val acc = ChatAccumulator(session).also { it.startIn(backgroundScope) }
        runCurrent()
        val slot = SessionSlot(session, acc, history)
        slot.prepareRegenerate()
        assertEquals(listOf(history.first()), slot.messages())
        session.events.emit(ChatEvent.Done("replacement"))
        runCurrent()
        assertEquals(2, slot.messages().size)
        assertEquals("replacement", (slot.messages().last() as ChatEntry.AssistantMessage).content)
    }

    @Test fun automaticSendRejectsThePreviousSessionAndWaitsForAnIdleTurn() {
        val session = FakeSession()
        val acc = ChatAccumulator(session)
        val slot = SessionSlot(session, acc)
        assertFalse(slot.sendMessage("task", expectedSessionId = "target"))
        assertTrue(slot.messages().isEmpty())
        session.sessionInfo.value = SessionInfo("target", null, false, 0)
        assertTrue(slot.sendMessage("task", expectedSessionId = "target"))
        assertFalse(slot.sendMessage("duplicate", expectedSessionId = "target"))
        assertEquals(listOf("task"), session.sentMessages)
    }

    @Test fun retryOnConnectedSessionRegeneratesWithoutDuplicatingUserMessage() {
        val session = FakeSession()
        val acc = ChatAccumulator(session)
        val slot = SessionSlot(session, acc)
        slot.sendMessage("question")
        acc.failTurn("Unauthorized")
        slot.regenerate()
        assertEquals(1, session.regenerationCount)
        assertEquals(1, slot.messages().filterIsInstance<ChatEntry.UserMessage>().size)
        assertTrue(acc.isGenerating.value)
        assertNull(acc.error.value)
    }

    @Test fun sessionSwitchVersionIsHandledOnlyOnceAcrossScreenRecreation() {
        val gate = SessionSwitchVersionGate()

        assertTrue(gate.tryAcquire(7))
        assertFalse(gate.tryAcquire(7))
        assertTrue(gate.tryAcquire(8))
    }

    private class FakeSession : ClawSeedSession {
        val sentMessages = mutableListOf<String>()
        var regenerationCount = 0
        override val connectionState = MutableStateFlow(ConnectionState.CONNECTED)
        override val sessionInfo = MutableStateFlow<SessionInfo?>(null)
        override val events = MutableSharedFlow<ChatEvent>()
        override val tools = ToolRegistry()
        override val gateway = GatewayClient("http://localhost")
        override suspend fun connect(sessionId: String?, persona: String?) = Unit
        override suspend fun disconnect() = Unit
        override fun sendMessage(content: String, debug: Boolean) { sentMessages.add(content) }
        override fun regenerate(debug: Boolean) { regenerationCount++ }
        override suspend fun abort() = Unit
        override fun close() = Unit
    }
}
