package dev.clawseed.sdk.android

import dev.clawseed.sdk.core.ClawSeedSession
import dev.clawseed.sdk.core.client.GatewayClient
import dev.clawseed.sdk.core.model.ChatEvent
import dev.clawseed.sdk.core.model.ConnectionState
import dev.clawseed.sdk.core.model.ResponseMetrics
import dev.clawseed.sdk.core.model.SessionInfo
import dev.clawseed.sdk.core.tool.ToolRegistry
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.advanceTimeBy
import org.junit.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class ChatAccumulatorTest {
    @Test
    fun debugToolsAreRetainedWithTheirMessageSnapshot() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        session.emit(ChatEvent.DebugPrompt("[]", 12, "[{\"name\":\"lookup\"}]", 34))
        runCurrent()
        val debug = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Debug>().single()
        assertEquals("[{\"name\":\"lookup\"}]", debug.toolsJson)
        assertEquals(34, debug.estimatedToolTokens)
        assertEquals(12, debug.estimatedTokens)
    }

    @Test
    fun debugPromptEstimatesAreAttachedToTheCompletedAssistantReply() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.DebugPrompt("[]", 12, "[]", 34))
        session.emit(ChatEvent.DebugPrompt("[updated]", 56, "[updated-tools]", 78))
        session.emit(ChatEvent.Done("answer"))
        runCurrent()

        val debug = accumulator.messages.value
            .filterIsInstance<AccumulatedMessage.Debug>()
            .single()
        assertEquals("[updated]", debug.messagesJson)
        assertEquals(56, debug.estimatedTokens)
        assertEquals(78, debug.estimatedToolTokens)
        val assistant = accumulator.messages.value
            .filterIsInstance<AccumulatedMessage.Assistant>()
            .single()
        assertEquals(56, assistant.estimatedTokens)
        assertEquals(78, assistant.estimatedToolTokens)
    }

    @Test
    fun doneReplacesThePreRequestEstimateWithTheLatestExactInput() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.DebugPrompt("[initial]", 9_930, "[]", 0))
        session.emit(
            ChatEvent.Done(
                "answer",
                ResponseMetrics(inputTokens = 40_000),
            ),
        )
        runCurrent()

        val debug = accumulator.messages.value
            .filterIsInstance<AccumulatedMessage.Debug>()
            .single()
        assertEquals(40_000, debug.estimatedTokens)
        val assistant = accumulator.messages.value
            .filterIsInstance<AccumulatedMessage.Assistant>()
            .single()
        assertEquals(40_000, assistant.estimatedTokens)
    }

    @Test
    fun contextCompactionProgressUpdatesOneDividerAndKeepsFinalTokenCounts() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.ContextCompactionStarted(20_100, 18_000, 3))
        session.emit(ChatEvent.ContextCompactionProgress(2, 3, "summarizing"))
        runCurrent()
        session.emit(ChatEvent.ContextCompactionCompleted(20_100, 7_500, 1_900))
        runCurrent()

        val compactions = accumulator.messages.value.filterIsInstance<AccumulatedMessage.ContextCompaction>()
        assertEquals(1, compactions.size)
        assertEquals(20_100, compactions.single().beforeTokens)
        assertEquals(2, compactions.single().completedChunks)
        assertEquals(3, compactions.single().totalChunks)
        assertEquals(7_500, compactions.single().afterTokens)
        assertEquals(1_900, compactions.single().summaryTokens)
        assertEquals("completed", compactions.single().stage)
    }


    @Test
    fun metricsStayWithTheirReplyAcrossResetRegenerationAndNextTurn() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        accumulator.addUserMessage("first question")
        val metrics = dev.clawseed.sdk.core.model.ResponseMetrics(inputTokens = 100, outputTokens = 20, cacheHitRatio = 0.0, elapsedMs = 2000)
        session.emit(ChatEvent.TextChunk("draft"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("answer", metrics))
        runCurrent()
        assertEquals(metrics, (accumulator.messages.value.last() as AccumulatedMessage.Assistant).metrics)
        accumulator.addUserMessage("next question")
        session.emit(ChatEvent.Done("next answer"))
        runCurrent()
        val assistants = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(metrics, assistants.first().metrics)
        assertEquals(null, assistants.last().metrics)
        accumulator.prepareRegenerate()
        session.emit(ChatEvent.Done("regenerated", metrics.copy(inputTokens = 200)))
        runCurrent()
        assertEquals(200L, (accumulator.messages.value.last() as AccumulatedMessage.Assistant).metrics?.inputTokens)
        accumulator.reset()
        accumulator.addUserMessage("fresh session")
        session.emit(ChatEvent.Done("fresh"))
        runCurrent()
        assertEquals(null, (accumulator.messages.value.last() as AccumulatedMessage.Assistant).metrics)
    }


    @Test
    fun rapidChunksPublishInBatchesWithoutLosingTheUnpublishedTail() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        session.emit(ChatEvent.TextChunk("first"))
        runCurrent()
        repeat(100) {
            session.emit(ChatEvent.TextChunk("x"))
            runCurrent()
        }
        assertEquals("first", accumulator.streamingContent.value)
        advanceTimeBy(40)
        runCurrent()
        assertEquals("first" + "x".repeat(100), accumulator.streamingContent.value)
        session.emit(ChatEvent.TextChunk("tail"))
        session.emit(ChatEvent.ThinkingChunk("reasoning"))
        session.emit(ChatEvent.Done(""))
        runCurrent()
        assertEquals("first" + "x".repeat(100) + "tail",
            (accumulator.messages.value.last() as AccumulatedMessage.Assistant).content)
        assertEquals("reasoning",
            accumulator.messages.value.filterIsInstance<AccumulatedMessage.Thinking>().single().content)
        advanceTimeBy(80)
        runCurrent()
        assertEquals("", accumulator.streamingContent.value)
        assertEquals("", accumulator.thinkingContent.value)
    }

    @Test
    fun stoppingCollectionCancelsPendingPublicationsAndReleasesSubscriber() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        session.emit(ChatEvent.TextChunk("draft"))
        runCurrent()
        accumulator.stop()
        session.emit(ChatEvent.Done("must not be collected"))
        advanceTimeBy(80)
        runCurrent()
        assertEquals(emptyList(), accumulator.messages.value)
        assertEquals("", accumulator.streamingContent.value)
    }

    @Test
    fun delayedAbortCleanupDoesNotResetTheNextTurn() {
        val accumulator = ChatAccumulator(FakeSession())
        accumulator.addUserMessage("first")
        val stoppedGeneration = accumulator.generationId
        accumulator.finishTurnIfCurrent(stoppedGeneration)
        assertEquals(false, accumulator.isGenerating.value)
        accumulator.addUserMessage("second")
        accumulator.finishTurnIfCurrent(stoppedGeneration)
        assertEquals(true, accumulator.isGenerating.value)
    }

    @Test
    fun errorBeforeFirstChunkEndsGenerationAndRetryClearsError() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        accumulator.addUserMessage("question")
        assertEquals(true, accumulator.isGenerating.value)
        session.emit(ChatEvent.Error("Unauthorized"))
        runCurrent()
        assertEquals(false, accumulator.isGenerating.value)
        assertEquals("Unauthorized", accumulator.error.value)
        accumulator.prepareRegenerate()
        assertEquals(true, accumulator.isGenerating.value)
        assertEquals(null, accumulator.error.value)
        session.emit(ChatEvent.Done(""))
        runCurrent()
        assertEquals(false, accumulator.isGenerating.value)
    }

    @Test
    fun stopBeforeFirstChunkAndStreamErrorBothClearBusyState() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        accumulator.addUserMessage("question")
        session.emit(ChatEvent.Aborted)
        runCurrent()
        assertEquals(false, accumulator.isGenerating.value)
        accumulator.addUserMessage("next")
        session.emit(ChatEvent.TextChunk("partial"))
        session.emit(ChatEvent.ThinkingChunk("thinking"))
        session.emit(ChatEvent.Error("Disconnected"))
        runCurrent()
        assertEquals(false, accumulator.isGenerating.value)
        assertEquals("", accumulator.streamingContent.value)
        assertEquals("", accumulator.thinkingContent.value)
    }

    @Test
    fun regenerateHistoryAndToolOnlyTurnsStayBusyUntilDone() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()
        accumulator.prepareRegenerate()
        assertEquals(true, accumulator.isGenerating.value)
        session.emit(ChatEvent.ToolCallStarted("id", "tool", kotlinx.serialization.json.buildJsonObject {}))
        session.emit(ChatEvent.ChunkReset)
        runCurrent()
        assertEquals(true, accumulator.isGenerating.value)
        session.emit(ChatEvent.Done("answer"))
        runCurrent()
        assertEquals(false, accumulator.isGenerating.value)
    }

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
    fun doneReplacesIncompleteChunksWithAuthoritativeFullResponse() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        session.emit(ChatEvent.TextChunk("second first "))
        session.emit(ChatEvent.TextChunk("third"))
        session.emit(ChatEvent.Done("first second third"))
        runCurrent()

        val assistantMessages = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, assistantMessages.size)
        assertEquals("first second third", assistantMessages.single().content)
        assertEquals("", accumulator.streamingContent.value)
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

    @Test
    fun secondTurnStartsWithFreshBuffersAfterDone() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        // First turn: user question → streaming → done
        accumulator.addUserMessage("first question")
        session.emit(ChatEvent.TextChunk("first answer"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("first answer"))
        runCurrent()

        val firstAssistant = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(1, firstAssistant.size)
        assertEquals("first answer", firstAssistant.single().content)
        assertEquals("", accumulator.streamingContent.value)

        // Second turn: buffers should be cleared by addUserMessage()
        accumulator.addUserMessage("second question")
        assertEquals("", accumulator.streamingContent.value)
        assertEquals("", accumulator.thinkingContent.value)

        session.emit(ChatEvent.TextChunk("second answer"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("second answer"))
        runCurrent()

        val allAssistant = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(2, allAssistant.size)
        assertEquals("second answer", allAssistant.last().content)
        assertEquals("", accumulator.streamingContent.value)
    }

    @Test
    fun secondTurnWithoutChunkResetStillWorks() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        // First turn: normal completion with ChunkReset + Done
        accumulator.addUserMessage("first question")
        session.emit(ChatEvent.TextChunk("first"))
        session.emit(ChatEvent.ChunkReset)
        session.emit(ChatEvent.Done("first answer"))
        runCurrent()

        // Second turn: streaming without ChunkReset (edge case — gateway always sends ChunkReset,
        // but verify the accumulator handles it defensively)
        accumulator.addUserMessage("second question")
        session.emit(ChatEvent.TextChunk("second answer"))
        session.emit(ChatEvent.Done("second answer"))
        runCurrent()

        val allAssistant = accumulator.messages.value.filterIsInstance<AccumulatedMessage.Assistant>()
        assertEquals(2, allAssistant.size)
        assertEquals("second answer", allAssistant.last().content)
    }

    @Test
    fun prepareRegenerateClearsStreamingBuffers() = runTest {
        val session = FakeSession()
        val accumulator = ChatAccumulator(session)
        accumulator.startIn(backgroundScope)
        runCurrent()

        accumulator.addUserMessage("question")
        session.emit(ChatEvent.TextChunk("draft text still streaming"))
        runCurrent()

        // Before regenerate, streaming content has partial text
        assertEquals("draft text still streaming", accumulator.streamingContent.value)

        // Prepare regenerate should clear streaming buffers
        accumulator.prepareRegenerate()
        assertEquals("", accumulator.streamingContent.value)
        assertEquals("", accumulator.thinkingContent.value)

        // Only the user message should remain
        val users = accumulator.messages.value.filterIsInstance<AccumulatedMessage.User>()
        assertEquals(1, users.size)
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

        override suspend fun connect(sessionId: String?, persona: String?) = Unit

        override suspend fun disconnect() = Unit

        override fun sendMessage(content: String, debug: Boolean) = Unit

        override fun regenerate(debug: Boolean) = Unit

        override suspend fun abort() = Unit

        suspend fun emit(event: ChatEvent) {
            mutableEvents.emit(event)
        }
    }
}
