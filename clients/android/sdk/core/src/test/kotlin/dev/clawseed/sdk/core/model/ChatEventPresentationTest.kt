package dev.clawseed.sdk.core.model

import kotlinx.serialization.json.Json
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class ChatEventPresentationTest {

    @Test
    fun toolResultParsesStructuredSearchPresentation() {
        val event = ChatEvent.parse(
            """
            {
              "type": "tool_result",
              "id": "call-1",
              "name": "web_search_tool",
              "output": "Search results for: mars",
              "presentation": {
                "version": 1,
                "blocks": [{
                  "type": "search_results",
                  "query": "mars",
                  "items": [{
                    "id": "nasa-mars",
                    "title": "NASA Mars Exploration",
                    "url": "https://mars.nasa.gov/",
                    "description": "Mission information",
                    "source": "NASA"
                  }]
                }]
              }
            }
            """.trimIndent(),
            Json { ignoreUnknownKeys = true },
        )

        val result = assertIs<ChatEvent.ToolCallCompleted>(event)
        val block = assertIs<ContentBlock.SearchResults>(result.presentation!!.blocks.single())
        assertEquals("mars", block.query)
        assertEquals("NASA Mars Exploration", block.items.single().title)
    }
}
