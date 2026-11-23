package dev.clawseed.demo.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.components.ClawSeedCard
import dev.clawseed.demo.ui.components.ClawSeedIconButton
import dev.clawseed.demo.ui.components.ClawSeedOutlinedButton
import dev.clawseed.demo.ui.components.ClawSeedSectionHeader
import dev.clawseed.demo.ui.components.ClawSeedTextField
import dev.clawseed.demo.ui.components.ClawSeedTopAppBar
import dev.clawseed.demo.ui.components.SettingsGroup
import dev.clawseed.demo.ui.components.SettingsSwitchRow
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.ToolInfo
import dev.clawseed.sdk.core.model.PluginInfo
import androidx.compose.ui.res.stringResource

@Composable
fun SkillManagementScreen(onBack: () -> Unit) {
    val viewModel: SettingsViewModel = viewModel()
    val state by viewModel.uiState.collectAsState()
    val snackbarHostState = remember { SnackbarHostState() }

    LaunchedEffect(state.error) {
        state.error?.let { snackbarHostState.showSnackbar(it); viewModel.clearError() }
    }
    LaunchedEffect(state.successMessage) {
        state.successMessage?.let { snackbarHostState.showSnackbar(it); viewModel.clearSuccess() }
    }
    BackHandler(enabled = state.editingSkill != null) { viewModel.closeSkillEditor() }

    Scaffold(
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
        topBar = {
            ClawSeedTopAppBar(
                title = { Text(stringResource(R.string.settings_skill_management)) },
                navigationIcon = {
                    ClawSeedIconButton(
                        onClick = { if (state.editingSkill != null) viewModel.closeSkillEditor() else onBack() },
                    ) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                    }
                },
                actions = {
                    ClawSeedIconButton(onClick = viewModel::refreshSkills, enabled = !state.isRefreshingSkills) {
                        if (state.isRefreshingSkills) {
                            CircularProgressIndicator(modifier = Modifier.size(18.dp))
                        } else {
                            Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.settings_refresh_skills))
                        }
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { padding ->
        if (state.editingSkill != null) {
            val skill = state.editingSkill!!
            SkillContentEditor(
                modifier = Modifier.fillMaxSize().padding(padding).padding(16.dp),
                skill = skill,
                content = state.skillContent,
                loading = state.isLoadingSkill,
                saving = state.isSavingSkill,
                onChange = viewModel::updateSkillContent,
                onSave = viewModel::saveSkill,
                onClose = viewModel::closeSkillEditor,
            )
        } else {
            LazyColumn(
                state = rememberLazyListState(),
                modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                item {
                    Text(
                        stringResource(R.string.settings_skill_management_desc, state.skills.size),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(vertical = 8.dp),
                    )
                }
                items(state.skills, key = { it.name }) { skill ->
                    SkillManagementCard(
                        skill = skill,
                        onToggle = { viewModel.toggleSkill(skill.name, it) },
                        onEdit = { viewModel.editSkill(skill) },
                    )
                }
            }
        }
    }
}

@Composable
private fun SkillManagementCard(skill: SkillInfo, onToggle: (Boolean) -> Unit, onEdit: () -> Unit) {
    ClawSeedCard(modifier = Modifier.fillMaxWidth(), onClick = onEdit) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            SettingsSwitchRow(
                title = if (skill.version.isBlank()) skill.name else "${skill.name}  v${skill.version}",
                subtitle = stringResource(R.string.settings_skill_edit_hint),
                checked = skill.enabled,
                onCheckedChange = onToggle,
            )
            if (skill.description.isNotBlank()) {
                Text(skill.description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
            if (skill.triggers.isNotEmpty()) {
                Text(skill.triggers.joinToString(" · "), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun SkillContentEditor(
    modifier: Modifier,
    skill: SkillInfo,
    content: String?,
    loading: Boolean,
    saving: Boolean,
    onChange: (String) -> Unit,
    onSave: () -> Unit,
    onClose: () -> Unit,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Column(modifier = Modifier.weight(1f)) {
                Text(skill.name, style = MaterialTheme.typography.titleMedium)
                Text(skill.description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            ClawSeedIconButton(onClick = onClose) {
                Icon(Icons.Default.Close, contentDescription = stringResource(R.string.common_close))
            }
        }
        if (loading) {
            Box(modifier = Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
        } else {
            ClawSeedTextField(
                value = content.orEmpty(),
                onValueChange = onChange,
                modifier = Modifier.fillMaxWidth().weight(1f),
                textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                label = { Text("SKILL.md") },
            )
            ClawSeedOutlinedButton(onClick = onSave, enabled = !saving, modifier = Modifier.fillMaxWidth()) {
                if (saving) CircularProgressIndicator(modifier = Modifier.size(18.dp))
                else Text(stringResource(R.string.settings_save_skill))
            }
        }
    }
}

@Composable
fun ToolManagementScreen(onBack: () -> Unit) {
    val viewModel: SettingsViewModel = viewModel()
    val state by viewModel.uiState.collectAsState()
    val snackbarHostState = remember { SnackbarHostState() }
    var expandedGroups by remember { mutableStateOf(emptySet<String>()) }
    LaunchedEffect(state.error) {
        state.error?.let { snackbarHostState.showSnackbar(it); viewModel.clearError() }
    }
    Scaffold(
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
        topBar = {
            ClawSeedTopAppBar(
                title = { Text(stringResource(R.string.settings_tool_management)) },
                navigationIcon = {
                    ClawSeedIconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                    }
                },
                actions = {
                    ClawSeedIconButton(onClick = viewModel::loadAll) {
                        Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.common_refresh))
                    }
                },
            )
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { padding ->
        val groups = state.tools
            .groupBy(::toolGroupKey)
            .toList()
            .sortedBy { toolGroupOrder(it.first) }
        LazyColumn(
            state = rememberLazyListState(),
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (state.plugins.isNotEmpty()) {
                item(key = "plugin-manifests-header") {
                    ClawSeedSectionHeader(
                        title = stringResource(R.string.settings_tool_group_plugin),
                        expanded = "plugin-manifests" in expandedGroups,
                        onToggle = {
                            expandedGroups = if ("plugin-manifests" in expandedGroups) {
                                expandedGroups - "plugin-manifests"
                            } else {
                                expandedGroups + "plugin-manifests"
                            }
                        },
                        subtitle = stringResource(R.string.settings_plugin_count, state.plugins.size),
                    )
                }
                if ("plugin-manifests" in expandedGroups) {
                    items(state.plugins, key = { "plugin:${it.pluginId}" }) { plugin ->
                        PluginManagementCard(
                            plugin = plugin,
                            onToggle = { viewModel.togglePlugin(plugin.pluginId, it) },
                        )
                    }
                }
            }
            groups.forEach { (group, tools) ->
                item(key = "header:$group") {
                    ClawSeedSectionHeader(
                        title = stringResource(toolGroupLabelRes(group)),
                        expanded = group in expandedGroups,
                        onToggle = {
                            expandedGroups = if (group in expandedGroups) expandedGroups - group else expandedGroups + group
                        },
                        subtitle = stringResource(R.string.settings_tool_group_count, tools.size),
                    )
                }
                if (group in expandedGroups) {
                    items(tools, key = { "tool:$group:${it.name}" }) { tool ->
                        ToolManagementCard(tool = tool, onToggle = { viewModel.toggleTool(tool.name, it) })
                    }
                }
            }
        }
    }
}

@Composable
private fun PluginManagementCard(plugin: PluginInfo, onToggle: (Boolean) -> Unit) {
    ClawSeedCard(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            SettingsSwitchRow(
                title = plugin.name?.takeIf { it.isNotBlank() } ?: plugin.pluginId,
                subtitle = listOfNotNull(
                    plugin.language?.takeIf { it.isNotBlank() },
                    plugin.version?.takeIf { it.isNotBlank() }?.let { "v$it" },
                    plugin.status,
                ).joinToString(" · "),
                checked = plugin.enabled,
                enabled = plugin.manifestValid,
                onCheckedChange = onToggle,
            )
            plugin.description?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            plugin.validationError?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.error, maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
private fun ToolManagementCard(tool: ToolInfo, onToggle: (Boolean) -> Unit) {
    ClawSeedCard(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            SettingsSwitchRow(
                title = tool.name,
                subtitle = toolSourceLabel(tool),
                checked = tool.enabled,
                onCheckedChange = onToggle,
            )
            if (tool.description.isNotBlank()) {
                Text(tool.description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

private fun toolSourceLabel(tool: ToolInfo): String = when (tool.sourceType) {
    "plugin" -> "Plugin${tool.source?.let { " · $it" }.orEmpty()}"
    "remote" -> "Remote${tool.source?.let { " · $it" }.orEmpty()}"
    "mcp" -> "MCP${tool.source?.let { " · $it" }.orEmpty()}"
    else -> "Built-in"
}

private fun toolGroupKey(tool: ToolInfo): String = when {
    tool.sourceType == "remote" && "__" in tool.name -> "third_party_app"
    tool.sourceType == "remote" -> "local_remote"
    tool.sourceType == "plugin" -> "plugin"
    tool.sourceType == "mcp" -> "mcp"
    tool.sourceType == "builtin" || tool.sourceType.isBlank() -> "builtin"
    else -> "other"
}

private fun toolGroupOrder(sourceType: String): Int = when (sourceType) {
    "builtin" -> 0
    "local_remote" -> 1
    "third_party_app" -> 2
    "plugin" -> 3
    "mcp" -> 4
    else -> 5
}

private fun toolGroupLabelRes(sourceType: String): Int = when (sourceType) {
    "builtin" -> R.string.settings_tool_group_builtin
    "local_remote" -> R.string.settings_tool_group_local_remote
    "third_party_app" -> R.string.settings_tool_group_third_party_app
    "plugin" -> R.string.settings_tool_group_plugin
    "mcp" -> R.string.settings_tool_group_mcp
    else -> R.string.settings_tool_group_other
}
