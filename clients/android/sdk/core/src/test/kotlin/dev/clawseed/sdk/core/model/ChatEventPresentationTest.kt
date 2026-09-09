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

    @Test
    fun toolResultParsesProfilePlanAndActions() {
        val event = ChatEvent.parse(
            """
            {
              "type":"tool_result","id":"call-2","name":"user_profile_change_plan","output":"preview",
              "presentation":{"version":1,"blocks":[{
                "type":"profile","title":"About me","summary":"Delete one item",
                "plan_id":"plan-1","profile_version":4,"requires_confirmation":true,
                "items":[{"id":"item-1","key":"preference.language","before":"Python","after":null}],
                "actions":[{"id":"confirm","label":"Confirm","command":"Apply profile plan plan-1","destructive":true}]
              }]}
            }
            """.trimIndent(),
            Json { ignoreUnknownKeys = true },
        )

        val result = assertIs<ChatEvent.ToolCallCompleted>(event)
        val block = assertIs<ContentBlock.Profile>(result.presentation!!.blocks.single())
        assertEquals("plan-1", block.planId)
        assertEquals(4, block.profileVersion)
        assertEquals("preference.language", block.items.single().key)
        assertEquals("Apply profile plan plan-1", block.actions.single().command)
    }
}
