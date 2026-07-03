package dev.clawseed.demo.ui.persona

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.R
import dev.clawseed.sdk.core.model.SkillInfo
import dev.clawseed.sdk.core.model.PersonaDetail
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.ToolInfo

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PersonaManagerScreen(
    onBack: () -> Unit,
    onStartChat: (String) -> Unit,
    initialPersona: String? = null,
    viewModel: PersonaViewModel = viewModel(),
) {
    val uiState by viewModel.uiState.collectAsState()

    LaunchedEffect(Unit) {
        viewModel.load()
    }

    LaunchedEffect(initialPersona) {
        if (!initialPersona.isNullOrBlank()) {
            viewModel.view(initialPersona)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        when {
                            uiState.editing != null -> stringResource(R.string.persona_edit_title)
                            uiState.viewing != null -> uiState.viewing!!.name
                            else -> stringResource(R.string.persona_manager_title)
                        },
                    )
                },
                navigationIcon = {
                    IconButton(onClick = {
                        when {
                            uiState.editing != null -> viewModel.closeEditor()
                            uiState.viewing != null && !initialPersona.isNullOrBlank() -> onBack()
                            uiState.viewing != null -> viewModel.closeEditor()
                            else -> onBack()
                        }
                    }) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.common_back))
                    }
                },
            )
        },
        floatingActionButton = {
            if (uiState.editing == null && uiState.viewing == null) {
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

                uiState.viewing != null -> PersonaDetailView(
                    detail = uiState.viewing!!,
                    skills = uiState.skills,
                    onStart = onStartChat,
                    onEdit = { viewModel.edit(uiState.viewing!!.name) },
                    onDuplicate = { viewModel.duplicate(uiState.viewing!!.toInfo()) },
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
                colors = CardDefaults.cardColors(containerColor = personaContainerColor(persona.name, persona.color)),
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    PersonaDot(
                        persona.name,
                        Modifier.size(34.dp),
                        showInitial = true,
                        avatar = persona.avatar,
                        color = persona.color,
                    )
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
                    IconButton(onClick = { onDuplicate(persona) }) {
                        Icon(Icons.Default.Add, contentDescription = stringResource(R.string.persona_duplicate))
                    }
                    IconButton(onClick = { confirmDelete = true }) {
                        Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.common_delete))
                    }
                }
            }
        }
    }
}

@Composable
private fun PersonaDetailView(
    detail: PersonaDetail,
    skills: List<SkillInfo>,
    onStart: (String) -> Unit,
    onEdit: () -> Unit,
    onDuplicate: () -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            PersonaIdentityHeader(detail)
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_llm_section)) {
                SettingLine(
                    label = stringResource(R.string.persona_model_label),
                    value = detail.model?.takeIf { it.isNotBlank() }
                        ?: stringResource(R.string.persona_model_inherit),
                )
                SettingLine(
                    label = stringResource(R.string.persona_thinking_override),
                    value = when (detail.thinkingEnabled) {
                        true -> stringResource(R.string.persona_thinking_on)
                        false -> stringResource(R.string.persona_thinking_off)
                        null -> stringResource(R.string.persona_thinking_inherit)
                    },
                )
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_memory_section)) {
                SettingLine(
                    label = stringResource(R.string.persona_memory_section),
                    value = detail.memoryNamespace?.takeIf { it.isNotBlank() }
                        ?: stringResource(R.string.persona_memory_shared),
                )
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_tools_section)) {
                SettingLine(
                    label = stringResource(R.string.persona_tools_section),
                    value = detail.allowedTools.takeIf { it.isNotEmpty() }
                        ?.joinToString(", ")
                        ?: stringResource(R.string.persona_tools_inherit),
                )
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_skills_section)) {
                val enabledSkills = skills
                    .filter { !detail.deniedSkills.contains(it.name) }
                    .map { it.name }
                SettingLine(
                    label = stringResource(R.string.persona_skills_enabled),
                    value = enabledSkills.takeIf { it.isNotEmpty() }
                        ?.joinToString(", ")
                        ?: stringResource(R.string.persona_skills_none_enabled),
                )
            }
        }
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedButton(onClick = onDuplicate, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Default.Add, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.persona_duplicate), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                OutlinedButton(onClick = onEdit, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Default.Edit, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.persona_edit), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Button(onClick = { onStart(detail.name) }, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Default.PlayArrow, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.persona_start), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
private fun PersonaIdentityHeader(detail: PersonaDetail) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = personaContainerColor(detail.name, detail.color)),
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.Top,
        ) {
            PersonaDot(
                detail.name,
                Modifier.size(52.dp),
                showInitial = true,
                avatar = detail.avatar,
                color = detail.color,
            )
            Spacer(Modifier.width(14.dp))
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    text = detail.name,
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    personaFeatureTags(detail).forEach { tag ->
                        FilterChip(
                            selected = false,
                            onClick = {},
                            label = {
                                Text(
                                    text = tag,
                                    style = MaterialTheme.typography.labelSmall,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            },
                            modifier = Modifier.height(26.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun personaFeatureTags(detail: PersonaDetail): List<String> {
    return buildList {
        addAll(personaTypeTags(detail))
        add(
            if (detail.hasSystemPrompt || detail.hasIdentity) {
                stringResource(R.string.persona_feature_custom_soul)
            } else {
                stringResource(R.string.persona_feature_global_soul)
            }
        )
        add(
            detail.model?.takeIf { it.isNotBlank() }
                ?: stringResource(R.string.persona_model_inherit)
        )
        add(
            when (detail.thinkingEnabled) {
                true -> stringResource(R.string.persona_feature_thinking_on)
                false -> stringResource(R.string.persona_feature_thinking_off)
                null -> stringResource(R.string.persona_feature_thinking_inherit)
            }
        )
        add(
            if (detail.memoryNamespace.isNullOrBlank()) {
                stringResource(R.string.persona_feature_shared_memory)
            } else {
                stringResource(R.string.persona_feature_private_memory)
            }
        )
        if (detail.allowedTools.isNotEmpty()) {
            add(stringResource(R.string.persona_feature_tools_count, detail.allowedTools.size))
        }
    }.distinct().take(6)
}

@Composable
private fun personaTypeTags(detail: PersonaDetail): List<String> {
    val text = "${detail.name} ${detail.systemPrompt.orEmpty()}".lowercase()
    val tags = mutableListOf<String>()
    if (listOf("翻译", "translate", "translation", "translator").any { text.contains(it) }) {
        tags.add(stringResource(R.string.persona_type_translation))
    }
    if (listOf("代码", "编程", "coding", "code", "developer", "program").any { text.contains(it) }) {
        tags.add(stringResource(R.string.persona_type_coding))
    }
    if (listOf("写作", "文案", "writing", "writer", "copy").any { text.contains(it) }) {
        tags.add(stringResource(R.string.persona_type_writing))
    }
    if (listOf("计划", "规划", "plan", "planner", "strategy").any { text.contains(it) }) {
        tags.add(stringResource(R.string.persona_type_planning))
    }
    if (listOf("研究", "搜索", "research", "analysis", "analyst").any { text.contains(it) }) {
        tags.add(stringResource(R.string.persona_type_research))
    }
    return tags.take(2)
}

@Composable
private fun SettingLine(label: String, value: String) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
        )
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

@Composable
private fun PersonaAppearanceSection(
    draft: PersonaDraft,
    onDraftChange: (PersonaDraft) -> Unit,
) {
    val context = LocalContext.current
    val avatarPicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            runCatching {
                context.contentResolver.takePersistableUriPermission(
                    uri,
                    Intent.FLAG_GRANT_READ_URI_PERMISSION,
                )
            }
            onDraftChange(draft.copy(avatar = uri.toString()))
        }
    }

    PersonaSection(title = stringResource(R.string.persona_appearance_section)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PersonaDot(
                draft.name.ifBlank { stringResource(R.string.persona_default) },
                Modifier.size(42.dp),
                showInitial = true,
                avatar = draft.avatar,
                color = draft.color,
            )
            Spacer(Modifier.width(12.dp))
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    text = stringResource(R.string.persona_avatar_label),
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    text = stringResource(R.string.persona_avatar_desc),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            OutlinedButton(onClick = { avatarPicker.launch(arrayOf("image/*")) }) {
                Text(stringResource(R.string.persona_avatar_choose))
            }
        }
        if (draft.avatar.isNotBlank()) {
            OutlinedButton(onClick = { onDraftChange(draft.copy(avatar = "", color = "")) }) {
                Text(stringResource(R.string.persona_avatar_clear))
            }
        }
    }
}

@Composable
private fun ThinkingModeSelector(
    value: Boolean?,
    onChange: (Boolean?) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(stringResource(R.string.persona_thinking_mode))
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            FilterChip(
                selected = value == null,
                onClick = { onChange(null) },
                label = { Text(stringResource(R.string.persona_thinking_inherit)) },
            )
            FilterChip(
                selected = value == true,
                onClick = { onChange(true) },
                label = { Text(stringResource(R.string.persona_thinking_on)) },
            )
            FilterChip(
                selected = value == false,
                onClick = { onChange(false) },
                label = { Text(stringResource(R.string.persona_thinking_off)) },
            )
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
    var soulExpanded by remember { mutableStateOf(false) }
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
            PersonaAppearanceSection(
                draft = draft,
                onDraftChange = onDraftChange,
            )
        }
        item {
            CollapsiblePersonaSection(
                title = stringResource(R.string.persona_soul_section),
                subtitle = if (draft.systemPrompt.isBlank()) {
                    stringResource(R.string.persona_soul_not_set_short)
                } else {
                    stringResource(R.string.persona_soul_customized)
                },
                expanded = soulExpanded,
                onToggle = { soulExpanded = !soulExpanded },
            ) {
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
                ThinkingModeSelector(
                    value = draft.thinkingEnabled,
                    onChange = { onDraftChange(draft.copy(thinkingEnabled = it)) },
                )
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
                    onSelectAll = { onDraftChange(draft.copy(allowedTools = tools.map { it.name }.toSet())) },
                    onSelectNone = { onDraftChange(draft.copy(allowedTools = emptySet())) },
                    onRestoreDefault = { onDraftChange(draft.copy(allowedTools = defaultAllowedTools(tools))) },
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
                    (skills.size - draft.deniedSkills.size).coerceAtLeast(0),
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
    onSelectAll: () -> Unit,
    onSelectNone: () -> Unit,
    onRestoreDefault: () -> Unit,
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
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            OutlinedButton(onClick = onSelectAll) {
                Text(stringResource(R.string.persona_tools_select_all))
            }
            OutlinedButton(onClick = onSelectNone) {
                Text(stringResource(R.string.persona_tools_select_none))
            }
            OutlinedButton(onClick = onRestoreDefault) {
                Text(stringResource(R.string.persona_tools_restore_default))
            }
        }
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

private fun PersonaDetail.toInfo(): PersonaInfo =
    PersonaInfo(
        name = name,
        isPersona = isPersona,
        hasIdentity = hasIdentity,
        hasSystemPrompt = hasSystemPrompt,
        memoryNamespace = memoryNamespace,
        allowedTools = allowedTools,
        deniedTools = deniedTools,
        deniedSkills = deniedSkills,
        model = model,
        thinkingEnabled = thinkingEnabled,
        avatar = avatar,
        color = color,
    )

private fun defaultAllowedTools(tools: List<ToolInfo>): Set<String> =
    tools.filter { it.sourceType == "builtin" }.map { it.name }.toSet()

private fun Set<String>.toggle(value: String): Set<String> =
    if (contains(value)) this - value else this + value
