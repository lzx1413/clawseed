package dev.clawseed.demo.ui.settings

import androidx.annotation.StringRes
import dev.clawseed.demo.R
import dev.clawseed.sdk.core.model.ToolInfo

internal enum class ToolSourceGroup(@param:StringRes val labelRes: Int) {
    PLUGIN(R.string.settings_tool_group_plugin),
    BUILTIN(R.string.settings_tool_group_builtin),
    LOCAL_REMOTE(R.string.settings_tool_group_local_remote),
    THIRD_PARTY_APP(R.string.settings_tool_group_third_party_app),
    MCP(R.string.settings_tool_group_mcp),
    OTHER(R.string.settings_tool_group_other),
}

internal fun ToolInfo.sourceGroup(): ToolSourceGroup = when (sourceType) {
    "plugin" -> ToolSourceGroup.PLUGIN
    "", "builtin" -> ToolSourceGroup.BUILTIN
    "remote" -> if ("__" in name) ToolSourceGroup.THIRD_PARTY_APP else ToolSourceGroup.LOCAL_REMOTE
    "mcp" -> ToolSourceGroup.MCP
    else -> ToolSourceGroup.OTHER
}

internal fun List<ToolInfo>.groupBySource() = groupBy { it.sourceGroup() }.toSortedMap()
