package dev.clawseed.demo.ui.profile

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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
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
    var deleteTarget by remember { mutableStateOf<UserProfileItem?>(null) }
    var rejectTarget by remember { mutableStateOf<UserProfileItem?>(null) }
    var confirmClear by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) { viewModel.load() }
    LaunchedEffect(uiState.error) {
        uiState.error?.let {
            snackbarHostState.showSnackbar(it)
            viewModel.clearError()
        }
    }

    deleteTarget?.let { item ->
        ConfirmationDialog(
            title = stringResource(R.string.profile_delete_title),
            message = stringResource(R.string.profile_delete_desc, item.key),
            confirmLabel = stringResource(R.string.common_delete),
            onDismiss = { deleteTarget = null },
            onConfirm = {
                deleteTarget = null
                viewModel.delete(item)
            },
        )
    }
    rejectTarget?.let { item ->
        ConfirmationDialog(
            title = stringResource(R.string.profile_reject_title),
            message = stringResource(R.string.profile_reject_desc, item.key),
            confirmLabel = stringResource(R.string.profile_reject),
            onDismiss = { rejectTarget = null },
            onConfirm = {
                rejectTarget = null
                viewModel.reject(item)
            },
        )
    }
    if (confirmClear) {
        ConfirmationDialog(
            title = stringResource(R.string.profile_clear_title),
            message = stringResource(R.string.profile_clear_desc),
            confirmLabel = stringResource(R.string.profile_clear),
            onDismiss = { confirmClear = false },
            onConfirm = {
                confirmClear = false
                viewModel.clear()
            },
        )
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
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.profile_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = stringResource(R.string.common_back),
                        )
                    }
                },
                actions = {
                    IconButton(onClick = viewModel::load, enabled = !uiState.isLoading && !uiState.isSaving) {
                        Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.common_refresh))
                    }
                    if (uiState.items.isNotEmpty()) {
                        IconButton(onClick = { confirmClear = true }, enabled = !uiState.isSaving) {
                            Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.profile_clear))
                        }
                    }
                },
            )
        },
        floatingActionButton = {
            if (!uiState.isSaving) {
                FloatingActionButton(onClick = viewModel::create) {
                    Icon(Icons.Default.Add, contentDescription = stringResource(R.string.profile_add))
                }
            }
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
    ) { padding ->
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            when {
                uiState.isLoading && uiState.items.isEmpty() -> CircularProgressIndicator(
                    modifier = Modifier.align(Alignment.Center),
                )

                uiState.items.isEmpty() -> EmptyProfileState(
                    onAdd = viewModel::create,
                    modifier = Modifier.align(Alignment.Center),
                )

                else -> UserProfileList(
                    items = uiState.items,
                    enabled = !uiState.isSaving,
                    onEdit = viewModel::edit,
                    onReject = { rejectTarget = it },
                    onDelete = { deleteTarget = it },
                )
            }

            if (uiState.isSaving) {
                CircularProgressIndicator(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .padding(24.dp)
                        .size(28.dp),
                )
            }
        }
    }
}

@Composable
private fun UserProfileList(
    items: List<UserProfileItem>,
    enabled: Boolean,
    onEdit: (UserProfileItem) -> Unit,
    onReject: (UserProfileItem) -> Unit,
    onDelete: (UserProfileItem) -> Unit,
) {
    val grouped = items.groupBy { it.category }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
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
                        enabled = enabled,
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
    enabled: Boolean,
    onEdit: () -> Unit,
    onReject: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (item.status == ProfileStatus.REJECTED) {
                MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)
            } else {
                MaterialTheme.colorScheme.surfaceVariant
            },
        ),
    ) {
        Column(
            modifier = Modifier.padding(start = 14.dp, top = 12.dp, end = 6.dp, bottom = 10.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
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
                IconButton(onClick = onEdit, enabled = enabled) {
                    Icon(Icons.Default.Edit, contentDescription = stringResource(R.string.profile_edit))
                }
                if (item.source == ProfileSource.INFERRED && item.status == ProfileStatus.ACTIVE) {
                    IconButton(onClick = onReject, enabled = enabled) {
                        Icon(Icons.Default.Close, contentDescription = stringResource(R.string.profile_reject))
                    }
                }
                IconButton(onClick = onDelete, enabled = enabled) {
                    Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.common_delete))
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
        modifier = modifier.padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.profile_empty_title), style = MaterialTheme.typography.titleMedium)
        Text(
            stringResource(R.string.profile_empty_desc),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Button(onClick = onAdd) {
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
    AlertDialog(
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
                OutlinedTextField(
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
                OutlinedTextField(
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
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    ProfileCategory.entries.forEach { category ->
                        FilterChip(
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
            TextButton(
                onClick = onSave,
                enabled = keyValid && !isSaving,
            ) {
                Text(stringResource(if (isSaving) R.string.common_saving else R.string.common_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, enabled = !isSaving) {
                Text(stringResource(R.string.common_cancel))
            }
        },
    )
}

@Composable
private fun ConfirmationDialog(
    title: String,
    message: String,
    confirmLabel: String,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { Text(message) },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(confirmLabel) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.common_cancel)) }
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
