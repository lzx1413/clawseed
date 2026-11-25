package dev.clawseed.demo.ui.persona

import dev.clawseed.demo.ui.settings.groupBySource
import dev.clawseed.sdk.core.model.ToolInfo
import dev.clawseed.sdk.core.model.PersonaDetail
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PersonaChoiceTest {
    @Test fun newPersonaSelectsAllBuiltinsOnly() {
        val tools = listOf(
            ToolInfo("builtin"), ToolInfo("disabled", enabled = false),
            ToolInfo("legacy", sourceType = ""), ToolInfo("plugin", sourceType = "plugin"),
            ToolInfo("local", sourceType = "remote"), ToolInfo("app__tool", sourceType = "remote"),
            ToolInfo("server", sourceType = "mcp"), ToolInfo("unknown", sourceType = "other"),
        )
        assertEquals(setOf("builtin", "disabled", "legacy"), defaultAllowedTools(tools))
        assertEquals(
            listOf(listOf("plugin"), listOf("builtin", "disabled", "legacy"), listOf("local"),
                listOf("app__tool"), listOf("server"), listOf("unknown")),
            tools.groupBySource().values.map { group -> group.map { it.name } },
        )
    }

    @Test fun togglingThinkingPreservesInheritedModelAndLegacyIdentity() {
        val stored = PersonaDetail(name = "reviewer", identity = JsonPrimitive("legacy"),
            allowedTools = listOf("*"), deniedTools = listOf("shell"), memoryNamespace = "shared-team")
        val displayed = PersonaDraft(name = stored.name, model = "default-model", provider = "default",
            systemPrompt = "legacy", allowedTools = setOf("read"), memoryMode = "isolated")
        val edited = stored.withChanges(displayed, displayed.copy(thinkingEnabled = true))

        assertEquals(true, edited.thinkingEnabled)
        assertNull(edited.provider)
        assertNull(edited.model)
        assertNull(edited.systemPrompt)
        assertEquals(stored.identity, edited.identity)
        assertEquals(stored.memoryNamespace, edited.memoryNamespace)
        assertEquals(stored.allowedTools, edited.allowedTools)
        assertEquals(stored.deniedTools, edited.deniedTools)
    }

    @Test fun clearingAllToolsPersistsAnEmptySelection() {
        val stored = PersonaDetail(name = "reviewer", allowedTools = listOf("read"))
        val before = PersonaDraft(name = stored.name, allowedTools = setOf("read"))
        assertEquals(emptyList<String>(), stored.withChanges(before, before.copy(allowedTools = emptySet())).allowedTools)
    }
}
