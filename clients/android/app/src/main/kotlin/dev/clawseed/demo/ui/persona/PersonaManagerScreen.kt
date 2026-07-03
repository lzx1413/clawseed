package dev.clawseed.demo.ui.persona

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.R
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.ToolInfo

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PersonaManagerScreen(
    onBack: () -> Unit,
    onStartChat: (String) -> Unit,
    viewModel: PersonaViewModel = viewModel(),
) {
    val uiState by viewModel.uiState.collectAsState()

    LaunchedEffect(Unit) {
        viewModel.load()
    }

    uiState.viewing?.let { detail ->
        AlertDialog(
            onDismissRequest = viewModel::closeEditor,
            title = { Text(detail.name) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(personaSummary(detail), style = MaterialTheme.typography.bodyMedium)
                    Text(
                        stringResource(R.string.persona_current_desc),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = viewModel::closeEditor) {
                    Text(stringResource(R.string.common_close))
                }
            },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        if (uiState.editing == null) stringResource(R.string.persona_manager_title)
                        else stringResource(R.string.persona_edit_title),
                    )
                },
                navigationIcon = {
                    IconButton(onClick = {
                        if (uiState.editing != null) viewModel.closeEditor() else onBack()
                    }) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                    }
                },
            )
        },
        floatingActionButton = {
            if (uiState.editing == null) {
                FloatingActionButton(onClick = viewModel::newPersona) {
                    Icon(Icons.Default.Add, contentDescription = stringResource(R.string.persona_create))
                }
            }
        },
    ) { padding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            when {
                uiState.editing != null -> PersonaEditor(
                    draft = uiState.editing!!,
                    tools = uiState.tools,
                    skills = uiState.skills,
                    availableModels = uiState.availableModels,
                    isSaving = uiState.isSaving,
                    onDraftChange = { viewModel.updateDraft { _ -> it } },
                    onSave = { viewModel.save() },
                    onSaveAndStart = { viewModel.save(onStartChat) },
                    onCancel = viewModel::closeEditor,
                )

                uiState.isLoading && uiState.personas.isEmpty() -> CircularProgressIndicator(
                    modifier = Modifier.align(Alignment.Center),
                )

                uiState.personas.isEmpty() -> EmptyPersonaState(
                    onCreate = viewModel::newPersona,
                )

                else -> PersonaList(
                    personas = uiState.personas,
                    onStart = onStartChat,
                    onView = viewModel::view,
                    onEdit = viewModel::edit,
                    onDuplicate = viewModel::duplicate,
                    onDelete = viewModel::delete,
                )
            }

            uiState.error?.let { error ->
                Card(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .padding(16.dp)
                        .fillMaxWidth(),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
                ) {
                    Text(
                        text = error,
                        modifier = Modifier.padding(12.dp),
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}

@Composable
private fun PersonaList(
    personas: List<PersonaInfo>,
    onStart: (String) -> Unit,
    onView: (String) -> Unit,
    onEdit: (String) -> Unit,
    onDuplicate: (PersonaInfo) -> Unit,
    onDelete: (String) -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(personas, key = { it.name }) { persona ->
            var confirmDelete by remember { mutableStateOf(false) }
            if (confirmDelete) {
                AlertDialog(
                    onDismissRequest = { confirmDelete = false },
                    title = { Text(stringResource(R.string.persona_delete_title)) },
                    text = { Text(stringResource(R.string.persona_delete_desc)) },
                    confirmButton = {
                        TextButton(onClick = { confirmDelete = false; onDelete(persona.name) }) {
                            Text(stringResource(R.string.common_delete))
                        }
                    },
                    dismissButton = {
                        TextButton(onClick = { confirmDelete = false }) {
                            Text(stringResource(R.string.common_cancel))
                        }
                    },
                )
            }

            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { onView(persona.name) },
                colors = CardDefaults.cardColors(containerColor = personaContainerColor(persona.name)),
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PersonaDot(persona.name, Modifier.size(34.dp), showInitial = true)
                    Spacer(Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            persona.name,
                            style = MaterialTheme.typography.titleSmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            personaSummary(persona),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    IconButton(onClick = { onStart(persona.name) }) {
                        Icon(Icons.Default.PlayArrow, contentDescription = stringResource(R.string.persona_start))
                    }
                    IconButton(onClick = { onEdit(persona.name) }) {
                        Icon(Icons.Default.Edit, contentDescription = stringResource(R.string.persona_edit))
                    }
                    IconButton(onClick = { confirmDelete = true }) {
                        Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.common_delete))
                    }
                }
                Row(
                    modifier = Modifier.padding(start = 58.dp, end = 12.dp, bottom = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    TextButton(onClick = { onDuplicate(persona) }) {
                        Text(stringResource(R.string.persona_duplicate))
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyPersonaState(onCreate: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            text = stringResource(R.string.persona_empty_title),
            style = MaterialTheme.typography.titleMedium,
        )
        Spacer(Modifier.height(8.dp))
        Text(
            text = stringResource(R.string.persona_empty_desc),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(16.dp))
        Button(onClick = onCreate) {
            Icon(Icons.Default.Add, contentDescription = null)
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.persona_create))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PersonaEditor(
    draft: PersonaDraft,
    tools: List<ToolInfo>,
    skills: List<SkillInfo>,
    availableModels: List<String>,
    isSaving: Boolean,
    onDraftChange: (PersonaDraft) -> Unit,
    onSave: () -> Unit,
    onSaveAndStart: () -> Unit,
    onCancel: () -> Unit,
) {
    var modelExpanded by remember { mutableStateOf(false) }
    var toolsExpanded by remember { mutableStateOf(false) }
    var skillsExpanded by remember { mutableStateOf(false) }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            OutlinedTextField(
                value = draft.name,
                onValueChange = { onDraftChange(draft.copy(name = it)) },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                label = { Text(stringResource(R.string.persona_name_label)) },
            )
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_soul_section)) {
                OutlinedTextField(
                    value = draft.systemPrompt,
                    onValueChange = { onDraftChange(draft.copy(systemPrompt = it)) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 220.dp),
                    textStyle = MaterialTheme.typography.bodySmall,
                    label = { Text(stringResource(R.string.persona_soul_label)) },
                    placeholder = { Text(stringResource(R.string.persona_soul_placeholder)) },
                )
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_llm_section)) {
                ExposedDropdownMenuBox(
                    expanded = modelExpanded,
                    onExpandedChange = { modelExpanded = it },
                ) {
                    OutlinedTextField(
                        value = draft.model.ifBlank { stringResource(R.string.persona_model_inherit) },
                        onValueChange = {},
                        modifier = Modifier
                            .fillMaxWidth()
                            .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
                        readOnly = true,
                        singleLine = true,
                        label = { Text(stringResource(R.string.persona_model_label)) },
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = modelExpanded) },
                    )
                    ExposedDropdownMenu(
                        expanded = modelExpanded,
                        onDismissRequest = { modelExpanded = false },
                    ) {
                        DropdownMenuItem(
                            text = { Text(stringResource(R.string.persona_model_inherit)) },
                            onClick = {
                                onDraftChange(draft.copy(model = ""))
                                modelExpanded = false
                            },
                        )
                        availableModels.forEach { model ->
                            DropdownMenuItem(
                                text = { Text(model, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                                onClick = {
                                    onDraftChange(draft.copy(model = model))
                                    modelExpanded = false
                                },
                            )
                        }
                    }
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(stringResource(R.string.persona_thinking_override))
                        Text(
                            stringResource(R.string.persona_thinking_override_desc),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(
                        checked = draft.thinkingEnabled != null,
                        onCheckedChange = { enabled ->
                            onDraftChange(draft.copy(thinkingEnabled = if (enabled) false else null))
                        },
                    )
                }
                if (draft.thinkingEnabled != null) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(stringResource(R.string.persona_thinking_enabled))
                            Text(
                                stringResource(R.string.persona_thinking_enabled_desc),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Switch(
                            checked = draft.thinkingEnabled == true,
                            onCheckedChange = { enabled ->
                                onDraftChange(draft.copy(thinkingEnabled = enabled))
                            },
                        )
                    }
                }
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_memory_section)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(stringResource(R.string.persona_isolated_memory))
                        Text(
                            stringResource(R.string.persona_isolated_memory_desc),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Switch(
                        checked = draft.memoryMode == "isolated",
                        onCheckedChange = {
                            onDraftChange(draft.copy(memoryMode = if (it) "isolated" else "shared"))
                        },
                    )
                }
            }
        }
        item {
            CollapsiblePersonaSection(
                title = stringResource(R.string.persona_tools_section),
                subtitle = stringResource(
                    R.string.persona_tools_summary,
                    draft.allowedTools.size,
                    tools.size,
                ),
                expanded = toolsExpanded,
                onToggle = { toolsExpanded = !toolsExpanded },
            ) {
                ToolSelector(
                    tools = tools,
                    allowed = draft.allowedTools,
                    onToggle = { tool ->
                        onDraftChange(
                            draft.copy(
                                allowedTools = draft.allowedTools.toggle(tool),
                            ),
                        )
                    },
                )
            }
        }
        item {
            CollapsiblePersonaSection(
                title = stringResource(R.string.persona_skills_section),
                subtitle = stringResource(
                    R.string.persona_skills_summary,
                    skills.size - draft.deniedSkills.size,
                    skills.size,
                ),
                expanded = skillsExpanded,
                onToggle = { skillsExpanded = !skillsExpanded },
            ) {
                SkillSelector(
                    skills = skills,
                    denied = draft.deniedSkills,
                    onToggle = { skill ->
                        onDraftChange(draft.copy(deniedSkills = draft.deniedSkills.toggle(skill)))
                    },
                )
            }
        }
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                OutlinedButton(onClick = onCancel, enabled = !isSaving) {
                    Text(stringResource(R.string.common_cancel))
                }
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = onSave, enabled = !isSaving) {
                    Text(if (isSaving) stringResource(R.string.common_saving) else stringResource(R.string.common_confirm))
                }
                Spacer(Modifier.width(8.dp))
                Button(onClick = onSaveAndStart, enabled = !isSaving) {
                    Text(stringResource(R.string.persona_save_start))
                }
            }
        }
    }
}

@Composable
private fun CollapsiblePersonaSection(
    title: String,
    subtitle: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onToggle),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(title, style = MaterialTheme.typography.titleSmall)
                    Text(
                        subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Icon(
                    imageVector = if (expanded) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown,
                    contentDescription = null,
                )
            }
            if (expanded) {
                content()
            }
        }
    }
}

@Composable
private fun PersonaSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            content()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ToolSelector(
    tools: List<ToolInfo>,
    allowed: Set<String>,
    onToggle: (String) -> Unit,
) {
    if (tools.isEmpty()) {
        Text(
            stringResource(R.string.persona_tools_empty),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        tools.forEach { tool ->
            PersonaToolCard(
                tool = tool,
                enabled = allowed.contains(tool.name),
                onToggle = { onToggle(tool.name) },
            )
        }
    }
}

@Composable
private fun PersonaToolCard(
    tool: ToolInfo,
    enabled: Boolean,
    onToggle: (Boolean) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = when (tool.sourceType) {
                "remote" -> MaterialTheme.colorScheme.tertiaryContainer
                "mcp" -> MaterialTheme.colorScheme.secondaryContainer
                else -> MaterialTheme.colorScheme.surfaceVariant
            },
        ),
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = tool.name,
                    style = MaterialTheme.typography.labelLarge,
                    color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f),
                    modifier = Modifier.weight(1f, fill = false),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(
                        selected = false,
                        onClick = {},
                        label = {
                            Text(
                                text = when (tool.sourceType) {
                                    "remote" -> "Remote"
                                    "mcp" -> "MCP"
                                    else -> "Built-in"
                                },
                                style = MaterialTheme.typography.labelSmall,
                            )
                        },
                        modifier = Modifier.height(24.dp),
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Switch(
                        checked = enabled,
                        onCheckedChange = onToggle,
                        modifier = Modifier.height(24.dp),
                    )
                }
            }
            if (tool.description.isNotBlank()) {
                Text(
                    text = tool.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun SkillSelector(
    skills: List<SkillInfo>,
    denied: Set<String>,
    onToggle: (String) -> Unit,
) {
    if (skills.isEmpty()) {
        Text(
            stringResource(R.string.persona_skills_empty),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        skills.forEach { skill ->
            PersonaSkillCard(
                skill = skill,
                enabled = !denied.contains(skill.name),
                onToggle = { onToggle(skill.name) },
            )
        }
    }
}

@Composable
private fun PersonaSkillCard(
    skill: SkillInfo,
    enabled: Boolean,
    onToggle: (Boolean) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (skill.version.isNotBlank()) "${skill.name}  v${skill.version}" else skill.name,
                    style = MaterialTheme.typography.labelLarge,
                    color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f),
                    modifier = Modifier.weight(1f, fill = false),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Switch(
                    checked = enabled,
                    onCheckedChange = onToggle,
                    modifier = Modifier.height(24.dp),
                )
            }
            if (skill.description.isNotBlank()) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = skill.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f)
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (skill.triggers.isNotEmpty()) {
                Spacer(modifier = Modifier.height(6.dp))
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    skill.triggers.forEach { trigger ->
                        FilterChip(
                            selected = false,
                            onClick = {},
                            label = {
                                Text(
                                    text = trigger,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                            modifier = Modifier.height(24.dp),
                        )
                    }
                }
            }
            if (skill.permissions.isNotEmpty()) {
                Spacer(modifier = Modifier.height(4.dp))
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    skill.permissions.forEach { perm ->
                        FilterChip(
                            selected = false,
                            onClick = {},
                            label = {
                                Text(
                                    text = perm,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                            modifier = Modifier.height(24.dp),
                        )
                    }
                }
            }
        }
    }
}

fun personaSummary(persona: PersonaInfo): String {
    val parts = mutableListOf<String>()
    if (persona.hasIdentity || persona.hasSystemPrompt) parts.add("Soul")
    persona.model?.takeIf { it.isNotBlank() }?.let { parts.add("model $it") }
    persona.thinkingEnabled?.let { parts.add(if (it) "thinking on" else "thinking off") }
    if (persona.memoryNamespace != null) parts.add("isolated memory")
    if (persona.allowedTools.isNotEmpty()) parts.add("allowed ${persona.allowedTools.size} tools")
    if (persona.deniedTools.isNotEmpty()) parts.add("blocked ${persona.deniedTools.size} tools")
    return parts.ifEmpty { listOf("Persona") }.joinToString(" · ")
}

fun personaSummary(persona: dev.clawseed.sdk.core.model.PersonaDetail): String {
    val parts = mutableListOf<String>()
    if (persona.hasIdentity || persona.hasSystemPrompt) parts.add("Soul")
    persona.model?.takeIf { it.isNotBlank() }?.let { parts.add("model: $it") }
    persona.thinkingEnabled?.let { parts.add(if (it) "thinking: on" else "thinking: off") }
    if (persona.memoryNamespace != null) parts.add("isolated memory: ${persona.memoryNamespace}")
    if (persona.allowedTools.isNotEmpty()) parts.add("allowed: ${persona.allowedTools.joinToString(", ")}")
    if (persona.deniedTools.isNotEmpty()) parts.add("blocked: ${persona.deniedTools.joinToString(", ")}")
    return parts.ifEmpty { listOf("Persona") }.joinToString("\n")
}

private fun Set<String>.toggle(value: String): Set<String> =
    if (contains(value)) this - value else this + value
