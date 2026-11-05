package dev.clawseed.demo.ui.persona

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import dev.clawseed.demo.ui.components.SettingsGroup
import dev.clawseed.demo.ui.components.ClawSeedButton
import dev.clawseed.demo.ui.components.ClawSeedOutlinedButton
import dev.clawseed.demo.ui.components.ClawSeedTextButton
import dev.clawseed.demo.ui.components.ClawSeedTextField
import dev.clawseed.demo.ui.components.ClawSeedChoiceChip
import dev.clawseed.demo.ui.components.ClawSeedIconButton
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import dev.clawseed.demo.ui.components.ClawSeedFormDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import dev.clawseed.demo.ui.components.ClawSeedTopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
            ClawSeedTopAppBar(
                title = {
                    Text(
                        when {
                            uiState.editing != null -> stringResource(R.string.persona_edit_title)
                            uiState.viewing != null -> personaDisplayName(uiState.viewing!!.name)
                            else -> stringResource(R.string.persona_manager_title)
                        },
                    )
                },
                navigationIcon = {
                    ClawSeedIconButton(onClick = {
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
                    availableProviders = uiState.availableProviders,
                    isFetchingModels = uiState.isFetchingModels,
                    isSaving = uiState.isSaving,
                    onFetchModels = viewModel::fetchModelsForEditingProvider,
                    onDraftChange = { viewModel.updateDraft { _ -> it } },
                    onSave = { viewModel.save() },
                    onSaveAndStart = { viewModel.save(onStartChat) },
                    onCancel = viewModel::closeEditor,
                )

                uiState.viewing != null -> PersonaDetailView(
                    detail = uiState.viewing!!,
                    tools = uiState.tools,
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
                dev.clawseed.demo.ui.components.ClawSeedCard(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .padding(16.dp)
                        .fillMaxWidth(),
                    containerColor = MaterialTheme.colorScheme.errorContainer,
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
        contentPadding = PaddingValues(start = 16.dp, top = 16.dp, end = 16.dp, bottom = 96.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(personas, key = { it.name }) { persona ->
            var confirmDelete by remember { mutableStateOf(false) }
            if (confirmDelete) {
                ClawSeedFormDialog(
                    onDismissRequest = { confirmDelete = false },
                    title = { Text(stringResource(R.string.persona_delete_title)) },
                    text = { Text(stringResource(R.string.persona_delete_desc)) },
                    confirmButton = {
                        ClawSeedTextButton(onClick = { confirmDelete = false; onDelete(persona.name) }) {
                            Text(stringResource(R.string.common_delete), color = MaterialTheme.colorScheme.error)
                        }
                    },
                    dismissButton = {
                        ClawSeedTextButton(onClick = { confirmDelete = false }) {
                            Text(stringResource(R.string.common_cancel))
                        }
                    },
                )
            }

            dev.clawseed.demo.ui.components.ClawSeedCard(
                modifier = Modifier.fillMaxWidth(),
                containerColor = personaCardColor(persona.name, persona.color),
                onClick = { onView(persona.name) },
            ) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        PersonaDot(persona.name, Modifier.size(40.dp), showInitial = true,
                            avatar = persona.avatar, color = persona.color)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(personaDisplayName(persona.name), style = MaterialTheme.typography.titleMedium)
                            Text(personaSummary(persona), style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3,
                                overflow = TextOverflow.Ellipsis)
                        }
                    }
                    androidx.compose.foundation.layout.FlowRow(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        ClawSeedOutlinedButton(onClick = { onStart(persona.name) }) {
                            Icon(Icons.Default.PlayArrow, null)
                            Spacer(Modifier.width(8.dp))
                            Text(stringResource(R.string.persona_start))
                        }
                        ClawSeedIconButton(onClick = { onEdit(persona.name) }, tooltip = stringResource(R.string.persona_edit)) {
                            Icon(Icons.Default.Edit, contentDescription = stringResource(R.string.persona_edit))
                        }
                        ClawSeedIconButton(onClick = { onDuplicate(persona) }, tooltip = stringResource(R.string.persona_duplicate)) {
                            Icon(Icons.Default.Add, contentDescription = stringResource(R.string.persona_duplicate))
                        }
                        if (persona.name != "default") {
                            ClawSeedIconButton(onClick = { confirmDelete = true }, dangerous = true,
                                tooltip = stringResource(R.string.common_delete)) {
                                Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.common_delete))
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PersonaDetailView(
    detail: PersonaDetail,
    tools: List<ToolInfo>,
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
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                ClawSeedButton(onClick = { onStart(detail.name) }, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Default.PlayArrow, contentDescription = null)
                    Spacer(Modifier.width(8.dp))
                    Text(stringResource(R.string.persona_start))
                }
                ClawSeedIconButton(onClick = onEdit) {
                    Icon(Icons.Default.Edit, contentDescription = stringResource(R.string.persona_edit))
                }
                ClawSeedIconButton(onClick = onDuplicate) {
                    Icon(Icons.Default.Add, contentDescription = stringResource(R.string.persona_duplicate))
                }
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_llm_section)) {
                SettingLine(
                    label = stringResource(R.string.persona_provider_label),
                    value = detail.provider?.takeIf { it.isNotBlank() }
                        ?: stringResource(R.string.persona_not_configured),
                )
                SettingLine(
                    label = stringResource(R.string.persona_model_label),
                    value = detail.model?.takeIf { it.isNotBlank() }
                        ?: stringResource(R.string.persona_model_not_configured),
                )
                SettingLine(
                    label = stringResource(R.string.persona_thinking_override),
                    value = when (detail.thinkingEnabled) {
                        true -> stringResource(R.string.persona_thinking_on)
                        false -> stringResource(R.string.persona_thinking_off)
                        null -> stringResource(R.string.persona_not_configured)
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
            Text(stringResource(R.string.persona_tools_section), style = MaterialTheme.typography.titleMedium)
            if (detail.allowedTools.isEmpty()) {
                Text(stringResource(R.string.persona_tools_none), style = MaterialTheme.typography.bodySmall)
            }
        }
        groupToolNames(detail.allowedTools).forEach { (group, names) ->
            item(key = "allowed:$group") {
                ToolPermissionGroup(stringResource(group.labelRes), names, tools)
            }
        }
        if (detail.deniedTools.isNotEmpty()) {
            item(key = "denied") {
                ToolPermissionGroup(stringResource(R.string.tools_blocked), detail.deniedTools, tools)
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
    }
}

@Composable
private fun ToolPermissionGroup(title: String, names: List<String>, tools: List<ToolInfo>) {
    var expanded by remember { mutableStateOf(false) }
    dev.clawseed.demo.ui.components.ClawSeedExpandableSection(
        title = stringResource(R.string.tools_group_count, title, names.size),
        expanded = expanded, onToggle = { expanded = !expanded },
    ) {
        SettingsGroup {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                names.forEach { name ->
                    dev.clawseed.demo.ui.components.SettingsRow(title = name,
                        subtitle = tools.find { it.name == name }?.description?.takeIf { it.isNotBlank() })
                }
            }
        }
    }
}

@Composable
private fun PersonaIdentityHeader(detail: PersonaDetail) {
    dev.clawseed.demo.ui.components.ClawSeedCard(
        modifier = Modifier.fillMaxWidth(),
        containerColor = personaCardColor(detail.name, detail.color),
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
                    text = personaDisplayName(detail.name),
                    style = MaterialTheme.typography.titleMedium,
                )
            }
        }
        FlowRow(modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    personaFeatureTags(detail).forEach { tag ->
                        ClawSeedChoiceChip(
                            selected = false,
                            onClick = {},
                            choiceType = dev.clawseed.demo.ui.components.ChoiceType.Information,
                            label = {
                                Text(
                                    text = tag,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                        )
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
                ?: stringResource(R.string.persona_not_configured)
        )
        add(
            when (detail.thinkingEnabled) {
                true -> stringResource(R.string.persona_feature_thinking_on)
                false -> stringResource(R.string.persona_feature_thinking_off)
                null -> stringResource(R.string.persona_not_configured)
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
            .fillMaxSize().verticalScroll(rememberScrollState())
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
        ClawSeedButton(onClick = onCreate) {
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
            val storedAvatar = PersonaAvatarStorage.saveAvatar(context, draft.name, uri)
            if (storedAvatar != null) {
                onDraftChange(draft.copy(avatar = storedAvatar))
            }
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
        }
        ClawSeedOutlinedButton(onClick = { avatarPicker.launch(arrayOf("image/*")) }) {
                Text(stringResource(R.string.persona_avatar_choose))
        }
        if (draft.avatar.isNotBlank()) {
            ClawSeedOutlinedButton(onClick = { onDraftChange(draft.copy(avatar = "", color = "")) }) {
                Text(stringResource(R.string.persona_avatar_clear))
            }
        }
    }
}

@Composable
private fun ThinkingModeSelector(
    value: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(stringResource(R.string.persona_thinking_mode))
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            ClawSeedChoiceChip(
                selected = value,
                onClick = { onChange(true) },
                label = { Text(stringResource(R.string.persona_thinking_on)) },
            )
            ClawSeedChoiceChip(
                selected = !value,
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
    availableProviders: List<dev.clawseed.sdk.core.model.ProviderInfo>,
    isFetchingModels: Boolean,
    isSaving: Boolean,
    onFetchModels: () -> Unit,
    onDraftChange: (PersonaDraft) -> Unit,
    onSave: () -> Unit,
    onSaveAndStart: () -> Unit,
    onCancel: () -> Unit,
) {
    var modelExpanded by remember { mutableStateOf(false) }
    var providerExpanded by remember { mutableStateOf(false) }
    var soulExpanded by remember { mutableStateOf(false) }
    var toolsExpanded by remember { mutableStateOf(false) }
    var skillsExpanded by remember { mutableStateOf(false) }

    LazyColumn(
        modifier = Modifier.fillMaxSize().imePadding(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            ClawSeedTextField(
                value = if (draft.originalName == "default") {
                    stringResource(R.string.persona_default_desc)
                } else {
                    draft.name
                },
                onValueChange = { onDraftChange(draft.copy(name = it)) },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                enabled = draft.originalName != "default",
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
                ClawSeedTextField(
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
                    expanded = providerExpanded,
                    onExpandedChange = { providerExpanded = it },
                ) {
                    ClawSeedTextField(
                        value = availableProviders.firstOrNull { it.id == draft.provider }?.name
                            ?.ifBlank { draft.provider } ?: draft.provider,
                        onValueChange = {},
                        modifier = Modifier
                            .fillMaxWidth()
                            .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
                        readOnly = true,
                        singleLine = true,
                        label = { Text("Provider") },
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = providerExpanded) },
                    )
                    ExposedDropdownMenu(
                        expanded = providerExpanded,
                        onDismissRequest = { providerExpanded = false },
                    ) {
                        availableProviders.forEach { provider ->
                            DropdownMenuItem(
                                text = { Text(provider.name.ifBlank { provider.id }) },
                                onClick = {
                                    onDraftChange(draft.copy(
                                        provider = provider.id,
                                        model = provider.models.firstOrNull() ?: provider.model.orEmpty(),
                                        vision = "auto",
                                    ))
                                    providerExpanded = false
                                },
                            )
                        }
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    ExposedDropdownMenuBox(
                        expanded = modelExpanded,
                        onExpandedChange = { modelExpanded = it },
                        modifier = Modifier.weight(1f),
                    ) {
                        ClawSeedTextField(
                            value = draft.model,
                            onValueChange = { onDraftChange(draft.copy(model = it, vision = "auto")) },
                            modifier = Modifier
                                .fillMaxWidth()
                                .menuAnchor(ExposedDropdownMenuAnchorType.PrimaryEditable),
                            singleLine = true,
                            label = { Text(stringResource(R.string.persona_model_label)) },
                            placeholder = { Text(stringResource(R.string.persona_model_placeholder)) },
                            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = modelExpanded) },
                        )
                        ExposedDropdownMenu(
                            expanded = modelExpanded,
                            onDismissRequest = { modelExpanded = false },
                        ) {
                            val providerModels = availableProviders
                                .firstOrNull { it.id == draft.provider }
                                ?.models
                                ?.ifEmpty { listOfNotNull(availableProviders.firstOrNull { it.id == draft.provider }?.model) }
                                ?: availableModels
                            providerModels.forEach { model ->
                                DropdownMenuItem(
                                    text = { Text(model, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                                    onClick = {
                                        onDraftChange(draft.copy(model = model, vision = "auto"))
                                        modelExpanded = false
                                    },
                                )
                            }
                        }
                    }
                    ClawSeedIconButton(onClick = onFetchModels, enabled = draft.provider.isNotBlank() && !isFetchingModels) {
                        if (isFetchingModels) {
                            CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                        } else {
                            Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.common_refresh))
                        }
                    }
                }
                dev.clawseed.demo.ui.components.VisionModeSelector(
                    value = draft.vision,
                    onChange = { onDraftChange(draft.copy(vision = it)) },
                )
                ThinkingModeSelector(
                    value = draft.thinkingEnabled,
                    onChange = { onDraftChange(draft.copy(thinkingEnabled = it)) },
                )
            }
        }
        item {
            PersonaSection(title = stringResource(R.string.persona_memory_section)) {
                dev.clawseed.demo.ui.components.SettingsSwitchRow(
                    title = stringResource(R.string.persona_isolated_memory),
                    subtitle = stringResource(R.string.persona_isolated_memory_desc),
                    checked = draft.memoryMode == "isolated",
                    onCheckedChange = { onDraftChange(draft.copy(memoryMode = if (it) "isolated" else "shared")) },
                )
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
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                ClawSeedOutlinedButton(onClick = onCancel, enabled = !isSaving) {
                    Text(stringResource(R.string.common_cancel))
                }
                ClawSeedOutlinedButton(onClick = onSave, enabled = !isSaving, loading = isSaving) {
                    Text(stringResource(R.string.common_confirm))
                }
                ClawSeedButton(onClick = onSaveAndStart, enabled = !isSaving) {
                    Text(stringResource(R.string.persona_save_start))
                }
            }
        }
    }
}

@Composable
private fun personaDisplayName(name: String): String =
    if (name == "default") stringResource(R.string.persona_default_desc) else name

@Composable
private fun CollapsiblePersonaSection(
    title: String,
    subtitle: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    dev.clawseed.demo.ui.components.ClawSeedExpandableSection(title, expanded, onToggle, subtitle) {
        SettingsGroup(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = content)
        }
    }
}

@Composable
private fun PersonaSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    SettingsGroup(modifier = Modifier.fillMaxWidth()) {
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
            ClawSeedOutlinedButton(onClick = onSelectAll) {
                Text(stringResource(R.string.persona_tools_select_all))
            }
            ClawSeedOutlinedButton(onClick = onSelectNone) {
                Text(stringResource(R.string.persona_tools_select_none))
            }
            ClawSeedOutlinedButton(onClick = onRestoreDefault) {
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
    dev.clawseed.demo.ui.components.ClawSeedCard(
        modifier = Modifier.fillMaxWidth(),
        containerColor = when (tool.sourceType) {
                "remote" -> MaterialTheme.colorScheme.tertiaryContainer
                "mcp" -> MaterialTheme.colorScheme.secondaryContainer
                else -> MaterialTheme.colorScheme.surfaceVariant
            },
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            dev.clawseed.demo.ui.components.SettingsSwitchRow(
                title = tool.name,
                subtitle = when (tool.sourceType) { "remote" -> "Remote"; "mcp" -> "MCP"; else -> "Built-in" },
                checked = enabled, onCheckedChange = onToggle,
            )
            if (tool.description.isNotBlank()) {
                Text(
                    text = tool.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
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
    SettingsGroup(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(12.dp)) {
            dev.clawseed.demo.ui.components.SettingsSwitchRow(
                title = if (skill.version.isNotBlank()) "${skill.name}  v${skill.version}" else skill.name,
                checked = enabled, onCheckedChange = onToggle,
            )
            if (skill.description.isNotBlank()) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = skill.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
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
                        ClawSeedChoiceChip(
                            selected = false,
                            onClick = {},
                            choiceType = dev.clawseed.demo.ui.components.ChoiceType.Information,
                            label = {
                                Text(
                                    text = trigger,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                            modifier = Modifier,
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
                        ClawSeedChoiceChip(
                            selected = false,
                            onClick = {},
                            choiceType = dev.clawseed.demo.ui.components.ChoiceType.Information,
                            label = {
                                Text(
                                    text = perm,
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            },
                            modifier = Modifier,
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
        provider = provider,
        allowedTools = allowedTools,
        deniedTools = deniedTools,
        deniedSkills = deniedSkills,
        model = model,
        thinkingEnabled = thinkingEnabled,
        vision = vision,
        avatar = avatar,
        color = color,
    )

private fun defaultAllowedTools(tools: List<ToolInfo>): Set<String> =
    tools.filter { it.sourceType == "builtin" }.map { it.name }.toSet()

private fun Set<String>.toggle(value: String): Set<String> =
    if (contains(value)) this - value else this + value
