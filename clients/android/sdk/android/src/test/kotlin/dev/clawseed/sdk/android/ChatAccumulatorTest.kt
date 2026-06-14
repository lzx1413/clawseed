package dev.clawseed.sdk.android

import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.runCurrent
import org.junit.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class ChatAccumulatorTest {

    @Test
    fun doneUsesFullResponseFallbackWhenNoChunksWereBuffered() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.Done("final answer"))
        runCurrent()

        val assistantMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, assistantMessages.size)
        assertEquals("final answer", assistantMessages.single().content)
    }

    @Test
    fun chunkResetAndDoneDoNotDuplicateAssistantMessage() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.TextChunk("hello"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("hello"))
        runCurrent()

        val assistantMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, assistantMessages.size)
        assertEquals("hello", assistantMessages.single().content)
    }

    @Test
    fun chunkResetDiscardsDraftAndDoneUsesAuthoritativeFullResponse() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        accumulator.addUserMessage("weather")
        session.emit(ChatEvent.TextChunk("让我先获取你的位置信息。"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("上海今日天气晴，20~27°C。"))
        runCurrent()

        val assistantMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, assistantMessages.size)
        assertEquals("上海今日天气晴，20~27°C。", assistantMessages.single().content)
        assertEquals("", accumulator.streamingContent.value)
    }

    @Test
    fun chunkResetKeepsThinkingUntilDoneFlushesIt() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.ThinkingChunk("analysis"))
        session.emit(ChatEvent.TextChunk("draft"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("final"))
        runCurrent()

        val thinkingMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Thinking>()
        val assistantMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, thinkingMessages.size)
        assertEquals("analysis", thinkingMessages.single().content)
        assertEquals(1, assistantMessages.size)
        assertEquals("final", assistantMessages.single().content)
    }

    @Test
    fun doneExtendsFlushedAssistantMessageWhenFullResponseHasMissingSuffix() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        accumulator.addUserMessage("question")
        session.emit(ChatEvent.TextChunk("hello"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("hello world"))
        runCurrent()

        val assistantMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, assistantMessages.size)
        assertEquals("hello world", assistantMessages.single().content)
    }

    @Test
    fun abortedAppendsSystemMessageAndClearsBuffers() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.TextChunk("partial"))
        session.emit(ChatEvent.ThinkingChunk("thinking"))
        session.emit(ChatEvent.Aborted)
        runCurrent()

        assertEquals("", accumulator.streamingContent.value)
        assertEquals("", accumulator.thinkingContent.value)

        val systemMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.System>()
        assertEquals(1, systemMessages.size)
        assertEquals("Generation aborted.", systemMessages.single().content)
    }

    private class FakeSession : ClawSeedSession {
        private val mutableEvents = MutableSharedFlow<ChatEvent>(extraBufferCapacity = 16)
        private val mutableConnectionState = MutableStateFlow(ConnectionState.CONNECTED)
        private val mutableSessionInfo = MutableStateFlow<SessionInfo?>(null)

        override val connectionState: StateFlow<ConnectionState> = mutableConnectionState
        override val sessionInfo: StateFlow<SessionInfo?> = mutableSessionInfo
        override val events: SharedFlow<ChatEvent> = mutableEvents
        override val tools: ToolRegistry = ToolRegistry()
        override val gateway: GatewayClient = GatewayClient("http://localhost")

        override suspend fun connect(sessionId: String?) = Unit

        override suspend fun disconnect() = Unit

        override fun sendMessage(content: String, debug: Boolean) = Unit

        override fun regenerate(debug: Boolean) = Unit

        override suspend fun abort() = Unit

        suspend fun emit(event: ChatEvent) {
            mutableEvents.emit(event)
        }
    }
}