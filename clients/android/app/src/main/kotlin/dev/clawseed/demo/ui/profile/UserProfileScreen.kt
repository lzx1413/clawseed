package dev.clawseed.demo.ui.profile

import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import dev.clawseed.demo.ui.components.ClawSeedButton
import dev.clawseed.demo.ui.components.ClawSeedTextButton
import dev.clawseed.demo.ui.components.ClawSeedTextField
import dev.clawseed.demo.ui.components.ClawSeedChoiceChip
import dev.clawseed.demo.ui.components.ClawSeedIconButton
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import dev.clawseed.demo.ui.components.ClawSeedTopAppBar
import dev.clawseed.demo.ui.components.ClawSeedAddAction
import dev.clawseed.demo.ui.components.ClawSeedFloatingIconButton
import dev.clawseed.demo.ui.components.ClawSeedCheckbox
import dev.clawseed.demo.ui.components.ClawSeedTab
import dev.clawseed.demo.ui.components.ClawSeedTabRow
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.semantics.Role
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.R
import dev.clawseed.sdk.core.model.ProfileCategory
import dev.clawseed.sdk.core.model.ProfileSource
import dev.clawseed.sdk.core.model.ProfileStatus
import dev.clawseed.sdk.core.model.UserProfileItem
import kotlin.math.roundToInt

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun UserProfileScreen(
    onBack: () -> Unit,
    viewModel: UserProfileViewModel = viewModel(),
) {
    val uiState by viewModel.uiState.collectAsState()
    val snackbarHostState = remember { SnackbarHostState() }
    var pendingDelete by remember { mutableStateOf<(() -> Unit)?>(null) }
    var deleteDescription by remember { mutableStateOf("") }
    pendingDelete?.let { action ->
        dev.clawseed.demo.ui.components.ClawSeedFormDialog(
            onDismissRequest = { pendingDelete = null },
            title = { Text(stringResource(R.string.profile_confirm_delete_title)) },
            text = { Text(deleteDescription) },
            confirmButton = {
                ClawSeedButton(onClick = { pendingDelete = null; action() }, dangerous = true) {
                    Text(stringResource(R.string.common_delete))
                }
            },
            dismissButton = { ClawSeedTextButton(onClick = { pendingDelete = null }) { Text(stringResource(R.string.common_cancel)) } },
        )
    }
    val clearDescription = stringResource(R.string.profile_confirm_clear)
    val selectedDescription = stringResource(R.string.profile_confirm_selected, uiState.selectedIds.size)
    val inferredDescription = stringResource(R.string.profile_confirm_inferred)
    val itemDescription = stringResource(R.string.profile_confirm_item)
    LaunchedEffect(Unit) { viewModel.load() }
    LaunchedEffect(uiState.error) {
        uiState.error?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.clearError()
        }
    }

    uiState.editing?.let { draft ->
        UserProfileEditorDialog(
            draft = draft,
            isSaving = uiState.isSaving,
            onChange = viewModel::updateDraft,
            onDismiss = viewModel::closeEditor,
            onSave = viewModel::save,
        )
    }

    Scaffold(
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
        topBar = {
            ClawSeedTopAppBar(
                title = { Text(stringResource(R.string.knowledge_title)) },
                navigationIcon = {
                    ClawSeedIconButton(onClick = onBack) {
                        Icon(
                            Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = stringResource(R.string.common_back),
                        )
                    }
                },
                actions = {
                    ClawSeedIconButton(
                        onClick = {
                            if (uiState.tab == KnowledgeTab.MEMORY) viewModel.loadMemories() else viewModel.load()
                        },
                        enabled = !uiState.isLoading && !uiState.isSaving,
                    ) {
                        Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.common_refresh))
                    }
                    if (uiState.tab == KnowledgeTab.ABOUT_ME && uiState.items.isNotEmpty()) {
                        ClawSeedIconButton(onClick = { deleteDescription = clearDescription; pendingDelete = viewModel::clear }, enabled = !uiState.isSaving) {
                            Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.profile_clear))
                        }
                    }
                },
            )
        },
        floatingActionButton = {
            if (!uiState.isSaving && uiState.tab == KnowledgeTab.ABOUT_ME) {
                ClawSeedAddAction(
                    onClick = viewModel::create,
                    contentDescription = stringResource(R.string.profile_add),
                )
            }
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            ClawSeedTabRow(selectedTabIndex = uiState.tab.ordinal) {
                ClawSeedTab(
                    selected = uiState.tab == KnowledgeTab.ABOUT_ME,
                    onClick = { viewModel.selectTab(KnowledgeTab.ABOUT_ME) },
                    text = { Text(stringResource(R.string.knowledge_about_me)) },
                )
                ClawSeedTab(
                    selected = uiState.tab == KnowledgeTab.MEMORY,
                    onClick = { viewModel.selectTab(KnowledgeTab.MEMORY) },
                    text = { Text(stringResource(R.string.knowledge_memory)) },
                )
            }
            val profileHeader: @Composable () -> Unit = {
                ProfileFilters(uiState, viewModel, onDeleteInferred = { deleteDescription = inferredDescription; pendingDelete = viewModel::deleteAllInferred })
                if (uiState.selectedIds.isNotEmpty()) {
                    FlowRow(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        ClawSeedButton(onClick = { deleteDescription = selectedDescription; pendingDelete = viewModel::deleteSelected }, dangerous = true) {
                            Text(stringResource(R.string.profile_delete_selected, uiState.selectedIds.size))
                        }
                        ClawSeedTextButton(onClick = viewModel::rejectSelected) {
                            Text(stringResource(R.string.profile_reject_selected))
                        }
                        ClawSeedIconButton(onClick = viewModel::clearSelection) {
                            Icon(Icons.Default.Close, contentDescription = stringResource(R.string.common_cancel))
                        }
                    }
                }
            }
            val filteredItems = uiState.items.filter { item ->
                uiState.categoryFilter?.let { item.category == it } ?: true
            }.filter { item ->
                uiState.sourceFilter?.let { item.source == it } ?: true
            }.filter { item ->
                uiState.statusFilter?.let { item.status == it } ?: true
            }
            when {
                uiState.isLoading && uiState.items.isEmpty() && uiState.memories.isEmpty() -> Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center,
                ) { CircularProgressIndicator() }

                uiState.tab == KnowledgeTab.MEMORY -> MemoryList(
                    memories = uiState.memories,
                    modifier = Modifier.weight(1f),
                )

                else -> UserProfileList(
                    items = filteredItems,
                    hasFilters = uiState.categoryFilter != null || uiState.sourceFilter != null || uiState.statusFilter != null,
                    onClearFilters = { viewModel.setCategoryFilter(null); viewModel.setSourceFilter(null); viewModel.setStatusFilter(null) },
                    header = profileHeader,
                    onAdd = viewModel::create,
                    selectedIds = uiState.selectedIds,
                    enabled = !uiState.isSaving,
                    modifier = Modifier.weight(1f),
                    onToggleSelection = viewModel::toggleSelection,
                    onEdit = viewModel::edit,
                    onReject = viewModel::reject,
                    onDelete = { item -> deleteDescription = itemDescription; pendingDelete = { viewModel.delete(item) } },
                )
            }

            if (uiState.isSaving) {
                Box(modifier = Modifier.fillMaxWidth().padding(12.dp), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(modifier = Modifier.size(28.dp))
                }
            }
        }
    }
}

@Composable
private fun ProfileFilters(uiState: UserProfileUiState, viewModel: UserProfileViewModel, onDeleteInferred: () -> Unit) {
    FlowRow(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        ClawSeedChoiceChip(
            selected = uiState.categoryFilter == null,
            onClick = { viewModel.setCategoryFilter(null) },
            label = { Text(stringResource(R.string.profile_filter_all)) },
        )
        ProfileCategory.entries.forEach { category ->
            ClawSeedChoiceChip(
                selected = uiState.categoryFilter == category,
                onClick = { viewModel.setCategoryFilter(category) },
                label = { Text(category.label()) },
            )
        }
        ClawSeedChoiceChip(
            selected = uiState.sourceFilter == ProfileSource.INFERRED,
            onClick = {
                viewModel.setSourceFilter(
                    ProfileSource.INFERRED.takeUnless { uiState.sourceFilter == it },
                )
            },
            label = { Text(stringResource(R.string.profile_source_inferred)) },
        )
        ClawSeedChoiceChip(
            selected = uiState.statusFilter == ProfileStatus.REJECTED,
            onClick = {
                viewModel.setStatusFilter(
                    ProfileStatus.REJECTED.takeUnless { uiState.statusFilter == it },
                )
            },
            label = { Text(stringResource(R.string.profile_status_rejected)) },
        )
        if (uiState.items.any { it.source == ProfileSource.INFERRED }) {
            ClawSeedTextButton(onClick = onDeleteInferred, dangerous = true) {
                Text(stringResource(R.string.profile_delete_inferred))
            }
        }
    }
}

@Composable
private fun UserProfileList(
    items: List<UserProfileItem>,
    hasFilters: Boolean,
    onClearFilters: () -> Unit,
    header: @Composable () -> Unit,
    onAdd: () -> Unit,
    selectedIds: Set<String>,
    enabled: Boolean,
    modifier: Modifier = Modifier,
    onToggleSelection: (String) -> Unit,
    onEdit: (UserProfileItem) -> Unit,
    onReject: (UserProfileItem) -> Unit,
    onDelete: (UserProfileItem) -> Unit,
) {
    val grouped = items.groupBy { it.category }
    LazyColumn(
        modifier = modifier.fillMaxWidth(),
        contentPadding = PaddingValues(start = 16.dp, top = 16.dp, end = 16.dp, bottom = 96.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item(key = "filters") { header() }
        if (items.isEmpty()) {
            item(key = "empty") {
                if (hasFilters) {
                    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(stringResource(R.string.profile_no_filter_results))
                        ClawSeedTextButton(onClick = onClearFilters) { Text(stringResource(R.string.profile_clear_filters)) }
                    }
                } else EmptyProfileState(onAdd, Modifier.fillMaxWidth())
            }
        }
        ProfileCategory.entries.forEach { category ->
            val categoryItems = grouped[category].orEmpty()
            if (categoryItems.isNotEmpty()) {
                item(key = "header-${category.name}") {
                    Text(
                        text = category.label(),
                        style = MaterialTheme.typography.titleMedium,
                        modifier = Modifier.padding(top = 8.dp, bottom = 2.dp),
                    )
                }
                items(categoryItems, key = { it.id }) { item ->
                    UserProfileItemCard(
                        item = item,
                        selected = item.id in selectedIds,
                        enabled = enabled,
                        onToggleSelection = { onToggleSelection(item.id) },
                        onEdit = { onEdit(item) },
                        onReject = { onReject(item) },
                        onDelete = { onDelete(item) },
                    )
                }
            }
        }
    }
}

@Composable
private fun UserProfileItemCard(
    item: UserProfileItem,
    selected: Boolean,
    enabled: Boolean,
    onToggleSelection: () -> Unit,
    onEdit: () -> Unit,
    onReject: () -> Unit,
    onDelete: () -> Unit,
) {
    dev.clawseed.demo.ui.components.ClawSeedCard(
        modifier = Modifier.fillMaxWidth(), selected = selected,
        muted = item.status == ProfileStatus.REJECTED,
    ) {
        Column(
            modifier = Modifier.padding(start = 14.dp, top = 12.dp, end = 6.dp, bottom = 10.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Row(modifier = Modifier.fillMaxWidth().toggleable(value = selected, enabled = enabled,
                role = Role.Checkbox, onValueChange = { onToggleSelection() }),
                verticalAlignment = Alignment.CenterVertically) {
                ClawSeedCheckbox(checked = selected, onCheckedChange = null, enabled = enabled)
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = item.key,
                        style = MaterialTheme.typography.labelLarge,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        text = UserProfileValueCodec.display(item.value),
                        style = MaterialTheme.typography.bodyLarge,
                        maxLines = 4,
                        overflow = TextOverflow.Ellipsis,
                    )
                }

            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                ClawSeedFloatingIconButton(
                    onClick = onEdit,
                    enabled = enabled,
                    contentDescription = stringResource(R.string.profile_edit),
                    tooltip = stringResource(R.string.profile_edit),
                ) {
                    Icon(Icons.Default.Edit, contentDescription = null, modifier = Modifier.size(20.dp))
                }
                if (item.source == ProfileSource.INFERRED && item.status == ProfileStatus.ACTIVE) {
                    ClawSeedFloatingIconButton(
                        onClick = onReject,
                        enabled = enabled,
                        dangerous = true,
                        contentDescription = stringResource(R.string.profile_reject),
                        tooltip = stringResource(R.string.profile_reject),
                    ) {
                        Icon(Icons.Default.Close, contentDescription = null, modifier = Modifier.size(20.dp))
                    }
                }
                ClawSeedFloatingIconButton(
                    onClick = onDelete,
                    enabled = enabled,
                    dangerous = true,
                    contentDescription = stringResource(R.string.common_delete),
                    tooltip = stringResource(R.string.common_delete),
                ) {
                    Icon(Icons.Default.Delete, contentDescription = null, modifier = Modifier.size(20.dp))
                }
            }
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                ProfileBadge(item.source.label())
                ProfileBadge(item.status.label())
                ProfileBadge(stringResource(R.string.profile_confidence, (item.confidence * 100).roundToInt()))
                ProfileBadge(stringResource(R.string.profile_updated, item.updatedAt.take(10)))
            }
        }
    }
}

@Composable
private fun MemoryList(memories: List<dev.clawseed.sdk.core.model.MemoryEntry>, modifier: Modifier = Modifier) {
    if (memories.isEmpty()) {
        Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
            Text(stringResource(R.string.memory_empty))
        }
        return
    }
    LazyColumn(
        modifier = modifier.fillMaxWidth(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(memories, key = { it.id }) { memory ->
            dev.clawseed.demo.ui.components.ClawSeedCard(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                    Text(memory.key, style = MaterialTheme.typography.labelLarge, fontFamily = FontFamily.Monospace)
                    Text(memory.content, style = MaterialTheme.typography.bodyMedium, maxLines = 5, overflow = TextOverflow.Ellipsis)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        ProfileBadge(memory.category)
                        ProfileBadge(memory.namespace)
                        ProfileBadge(memory.timestamp.take(10))
                    }
                }
            }
        }
    }
}

@Composable
private fun ProfileBadge(text: String) {
    Surface(
        color = MaterialTheme.colorScheme.surface,
        shape = MaterialTheme.shapes.extraSmall,
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 7.dp, vertical = 3.dp),
        )
    }
}

@Composable
private fun EmptyProfileState(onAdd: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.profile_empty_title), style = MaterialTheme.typography.titleMedium)
        Text(
            stringResource(R.string.profile_empty_desc),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        ClawSeedButton(onClick = onAdd) {
            Icon(Icons.Default.Add, contentDescription = null)
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.profile_add))
        }
    }
}

@Composable
private fun UserProfileEditorDialog(
    draft: UserProfileDraft,
    isSaving: Boolean,
    onChange: ((UserProfileDraft) -> UserProfileDraft) -> Unit,
    onDismiss: () -> Unit,
    onSave: () -> Unit,
) {
    val keyValid = isUserProfileKeyValid(draft.key.trim())
    dev.clawseed.demo.ui.components.ClawSeedFormDialog(
        onDismissRequest = { if (!isSaving) onDismiss() },
        title = {
            Text(
                stringResource(
                    if (draft.originalId == null) R.string.profile_add else R.string.profile_edit,
                ),
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                ClawSeedTextField(
                    value = draft.key,
                    onValueChange = { value -> onChange { it.copy(key = value) } },
                    label = { Text(stringResource(R.string.profile_key)) },
                    readOnly = draft.originalId != null,
                    enabled = !isSaving,
                    isError = draft.key.isNotEmpty() && !keyValid,
                    supportingText = if (draft.key.isNotEmpty() && !keyValid) {
                        { Text(stringResource(R.string.profile_key_invalid)) }
                    } else {
                        null
                    },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                ClawSeedTextField(
                    value = draft.value,
                    onValueChange = { value -> onChange { it.copy(value = value) } },
                    label = { Text(stringResource(R.string.profile_value)) },
                    enabled = !isSaving,
                    minLines = 2,
                    maxLines = 6,
                    modifier = Modifier.fillMaxWidth(),
                )
                Text(stringResource(R.string.profile_category), style = MaterialTheme.typography.labelLarge)
                FlowRow(
                    modifier = Modifier.selectableGroup(),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    ProfileCategory.entries.forEach { category ->
                        ClawSeedChoiceChip(
                            selected = draft.category == category,
                            onClick = { onChange { it.copy(category = category) } },
                            label = { Text(category.label()) },
                            enabled = !isSaving,
                        )
                    }
                }
            }
        },
        confirmButton = {
            ClawSeedButton(
                onClick = onSave,
                loading = isSaving,
                enabled = keyValid && !isSaving,
            ) {
                Text(stringResource(R.string.common_confirm))
            }
        },
        dismissButton = {
            ClawSeedTextButton(onClick = onDismiss, enabled = !isSaving) {
                Text(stringResource(R.string.common_cancel))
            }
        },
    )
}

@Composable
private fun ProfileCategory.label(): String = stringResource(
    when (this) {
        ProfileCategory.IDENTITY -> R.string.profile_category_identity
        ProfileCategory.PREFERENCE -> R.string.profile_category_preference
        ProfileCategory.EXPERTISE -> R.string.profile_category_expertise
        ProfileCategory.GOAL -> R.string.profile_category_goal
        ProfileCategory.CONSTRAINT -> R.string.profile_category_constraint
        ProfileCategory.ACCESSIBILITY -> R.string.profile_category_accessibility
    },
)

@Composable
private fun ProfileSource.label(): String = stringResource(
    when (this) {
        ProfileSource.EXPLICIT -> R.string.profile_source_explicit
        ProfileSource.INFERRED -> R.string.profile_source_inferred
        ProfileSource.IMPORTED -> R.string.profile_source_imported
    },
)

@Composable
private fun ProfileStatus.label(): String = stringResource(
    when (this) {
        ProfileStatus.ACTIVE -> R.string.profile_status_active
        ProfileStatus.SUPERSEDED -> R.string.profile_status_superseded
        ProfileStatus.REJECTED -> R.string.profile_status_rejected
    },
)
