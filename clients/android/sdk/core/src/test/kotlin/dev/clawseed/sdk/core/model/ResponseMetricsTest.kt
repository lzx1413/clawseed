package dev.clawseed.sdk.core.model

import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull

class ResponseMetricsTest {
    @Test
    fun debugPromptPreservesSeparateToolsAndAcceptsLegacyFrames() {
        val frame = """{"type":"debug_prompt","messages":"[]","estimated_tokens":12,"tools":"[]","estimated_tool_tokens":34}"""
        val event = assertIs<ChatEvent.DebugPrompt>(ChatEvent.parse(frame, Json))
        assertEquals("[]", event.toolsJson)
        assertEquals(34, event.estimatedToolTokens)
        val legacy = assertIs<ChatEvent.DebugPrompt>(ChatEvent.parse("""{"type":"debug_prompt","messages":"[]","estimated_tokens":12}""", Json))
        assertNull(legacy.toolsJson)
        assertEquals(0, legacy.estimatedToolTokens)
    }

    private val json = Json { ignoreUnknownKeys = true }
    private val metrics = """{"input_tokens":1000,"output_tokens":50,"cached_input_tokens":0,"cache_hit_ratio":0.0,"output_tokens_per_second":25.0,"elapsed_ms":2500,"future_field":true}"""

    @Test
    fun doneAndHistoryPreserveKnownZeroAndLargeCounts() {
        val done = assertIs<ChatEvent.Done>(ChatEvent.parse("""{"type":"done","full_response":"answer","metrics":$metrics}""", json))
        val history = json.decodeFromString<SessionMessage>("""{"role":"assistant","content":"answer","metrics":$metrics}""")
        assertEquals(done.metrics, history.metrics)
        assertEquals(1000L, done.metrics?.inputTokens)
        assertEquals(0L, done.metrics?.cachedInputTokens)
        assertEquals(0.0, done.metrics?.cacheHitRatio)
        assertEquals(2500L, done.metrics?.elapsedMs)
    }

    @Test
    fun legacyMissingAndMalformedMetricsDoNotBreakReply() {
        for (extra in listOf("", ",\"metrics\":null", ",\"metrics\":\"bad\"")) {
            val done = assertIs<ChatEvent.Done>(ChatEvent.parse("""{"type":"done","full_response":"answer"$extra}""", json))
            assertEquals("answer", done.fullResponse)
            assertNull(done.metrics)
        }
        val unknown = assertIs<ChatEvent.Done>(ChatEvent.parse("""{"type":"done","metrics":{"elapsed_ms":10}}""", json))
        assertNull(unknown.metrics?.inputTokens)
        assertNull(unknown.metrics?.cacheHitRatio)
    }

    @Test
    fun contextCompactionEventsDecodeProgressAndTokenCounts() {
        val started = assertIs<ChatEvent.ContextCompactionStarted>(ChatEvent.parse(
            """{"type":"context_compaction_started","before_tokens":20100,"source_tokens":18000,"total_chunks":3}""",
            json,
        ))
        assertEquals(20_100, started.beforeTokens)
        assertEquals(3, started.totalChunks)

        val progress = assertIs<ChatEvent.ContextCompactionProgress>(ChatEvent.parse(
            """{"type":"context_compaction_progress","completed_chunks":2,"total_chunks":3,"stage":"summarizing"}""",
            json,
        ))
        assertEquals(2, progress.completedChunks)
        assertEquals("summarizing", progress.stage)

        val completed = assertIs<ChatEvent.ContextCompactionCompleted>(ChatEvent.parse(
            """{"type":"context_compaction_completed","before_tokens":20100,"after_tokens":7500,"summary_tokens":1900}""",
            json,
        ))
        assertEquals(7_500, completed.afterTokens)
        assertEquals(1_900, completed.summaryTokens)
    }
}
