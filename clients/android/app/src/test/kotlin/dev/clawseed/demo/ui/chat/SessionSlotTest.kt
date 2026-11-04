package dev.clawseed.demo.ui.chat

import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.ToolCallInfo
import dev.clawseed.sdk.android.ChatAccumulator
import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.model.SessionMessage
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
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

    @Test fun interleavedThinkingAndToolsRemainInTimelineOrder() = runTest {
        val session = FakeSession()
        val acc = ChatAccumulator(session).also { it.startIn(backgroundScope) }
        runCurrent()
        val slot = SessionSlot(session, acc)

        slot.sendMessage("question")
        session.events.emit(ChatEvent.ThinkingChunk("before tool"))
        session.events.emit(
            ChatEvent.ToolCallStarted(
                id = "call-1",
                name = "lookup",
                args = kotlinx.serialization.json.buildJsonObject {},
            ),
        )
        session.events.emit(ChatEvent.ToolCallCompleted("call-1", "lookup", "result"))
        session.events.emit(ChatEvent.ThinkingChunk("after tool"))
        session.events.emit(ChatEvent.Done("answer"))
        runCurrent()

        assertEquals(
            listOf(
                ChatEntry.UserMessage::class,
                ChatEntry.TurnActivity::class,
                ChatEntry.AssistantMessage::class,
            ),
            slot.messages().map { it::class },
        )
        val activity = slot.messages()[1] as ChatEntry.TurnActivity
        assertEquals(1, activity.toolCount)
        assertEquals(
            listOf(ChatEntry.Thinking::class, ChatEntry.ToolInvocations::class, ChatEntry.Thinking::class),
            activity.activities.map { it::class },
        )
        assertEquals("result", (activity.activities[1] as ChatEntry.ToolInvocations).invocations.single().toolResult)
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

    @Test fun completedThinkingWithoutToolsStillCollapsesWithZeroToolCount() = runTest {
        val session = FakeSession()
        val acc = ChatAccumulator(session).also { it.startIn(backgroundScope) }
        runCurrent()
        val slot = SessionSlot(session, acc)

        slot.sendMessage("question")
        session.events.emit(ChatEvent.ThinkingChunk("thinking"))
        session.events.emit(ChatEvent.Done("answer"))
        runCurrent()

        val activity = slot.messages().filterIsInstance<ChatEntry.TurnActivity>().single()
        assertEquals(0, activity.toolCount)
        assertEquals("thinking", (activity.activities.single() as ChatEntry.Thinking).content)
    }

    @Test fun legacyStructuredAssistantContentRestoresThinkingAndTools() {
        val messages = listOf(
            SessionMessage(role = "user", content = "question"),
            SessionMessage(
                role = "assistant",
                content = """{"tool_calls":[{"id":"call-1","name":"lookup","arguments":"{\"q\":\"x\"}"}],"reasoning_content":"thinking"}""",
            ),
            SessionMessage(
                role = "tool",
                content = """{"tool_call_id":"call-1","name":"lookup","content":"result"}""",
            ),
            SessionMessage(role = "assistant", content = "answer"),
        )

        val entries = mapPersistedSessionMessages(messages)
        assertEquals(listOf(ChatEntry.UserMessage::class, ChatEntry.TurnActivity::class, ChatEntry.AssistantMessage::class), entries.map { it::class })
        val activity = entries[1] as ChatEntry.TurnActivity
        assertEquals(1, activity.toolCount)
        assertEquals(ChatEntry.Thinking::class, activity.activities.first()::class)
        val invocation = activity.activities.filterIsInstance<ChatEntry.ToolInvocations>().single().invocations.single()
        assertEquals("lookup", invocation.toolName)
        assertEquals("{\"q\":\"x\"}", invocation.toolArgs)
        assertEquals("result", invocation.toolResult)
    }

    @Test fun persistedTimelineRestoresActivitiesBeforeAssistant() {
        val entries = mapPersistedSessionMessages(
            listOf(
                SessionMessage(role = "user", content = "question"),
                SessionMessage(
                    role = "assistant",
                    content = "answer",
                    timeline = Json.parseToJsonElement(
                        """{"version":1,"events":[{"kind":"thinking","content":"think"},{"kind":"tool_call","id":"call-1","name":"lookup","args":"{}"},{"kind":"tool_result","id":"call-1","name":"lookup","output":"result"}]}""",
                    ),
                ),
            ),
        )
        val activity = entries[1] as ChatEntry.TurnActivity
        assertEquals(1, activity.toolCount)
        assertEquals("think", (activity.activities.first() as ChatEntry.Thinking).content)
        assertEquals("result", (activity.activities[1] as ChatEntry.ToolInvocations).invocations.single().toolResult)
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
