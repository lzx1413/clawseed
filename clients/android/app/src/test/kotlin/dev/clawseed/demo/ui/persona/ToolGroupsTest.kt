package dev.clawseed.demo.ui.persona

import org.junit.Assert.*
import org.junit.Test

class ToolGroupsTest {
    @Test fun categorizesKnownToolsAndPreservesUnknownTools() {
        val names = listOf("app__camera", "file_read", "web_search", "memory_recall", "cron_list", "new_extension")
        val groups = groupToolNames(names)
        assertEquals(listOf("app__camera"), groups[ToolGroup.DEVICE])
        assertEquals(listOf("file_read"), groups[ToolGroup.FILES])
        assertEquals(listOf("web_search"), groups[ToolGroup.NETWORK])
        assertEquals(listOf("memory_recall"), groups[ToolGroup.MEMORY])
        assertEquals(listOf("cron_list"), groups[ToolGroup.TASKS])
        assertEquals(listOf("new_extension"), groups[ToolGroup.OTHER])
        assertEquals(names.toSet(), groups.values.flatten().toSet())
    }

    @Test fun groupsAreStableAndDeduplicatedWithoutMutatingTheInput() {
        val names = listOf("file_write", "file_read", "file_read")
        assertEquals(listOf("file_read", "file_write"), groupToolNames(names)[ToolGroup.FILES])
        assertEquals(3, names.size)
        assertTrue(groupToolNames(emptyList()).isEmpty())
    }
}
