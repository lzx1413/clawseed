package dev.clawseed.demo.ui.persona

import androidx.annotation.StringRes
import dev.clawseed.demo.R

internal enum class ToolGroup(@param:StringRes val labelRes: Int) {
    FILES(R.string.tools_group_files),
    NETWORK(R.string.tools_group_network),
    MEMORY(R.string.tools_group_memory),
    TASKS(R.string.tools_group_tasks),
    DEVICE(R.string.tools_group_device),
    OTHER(R.string.tools_group_other),
}

internal fun groupToolNames(names: List<String>): Map<ToolGroup, List<String>> = names.distinct().sorted()
    .groupBy { name ->
        when {
            name.startsWith("app__") -> ToolGroup.DEVICE
            name.startsWith("file_") || name in setOf("glob_search", "content_search", "pdf_read", "shell", "git_operations") -> ToolGroup.FILES
            name.startsWith("web_") || name == "http_request" -> ToolGroup.NETWORK
            name.startsWith("memory_") || name.startsWith("knowledge") -> ToolGroup.MEMORY
            name.startsWith("cron_") || name.startsWith("alarm_") -> ToolGroup.TASKS
            else -> ToolGroup.OTHER
        }
    }.toSortedMap(compareBy { it.ordinal })
