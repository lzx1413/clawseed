package dev.clawseed.demo.ui.drawer

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.BuildConfig
import dev.clawseed.demo.data.ChatSession

@Composable
fun SessionDrawer(
    currentSessionId: String?,
    onSelectSession: (String) -> Unit,
    onSettings: () -> Unit,
    isDrawerOpen: Boolean = false,
    refreshKey: Int = 0,
    viewModel: SessionsViewModel = viewModel(),
) {
    val uiState by viewModel.uiState.collectAsState()
    var showAbout by remember { mutableStateOf(false) }

    // Refresh session list every time drawer opens
    LaunchedEffect(isDrawerOpen, refreshKey) {
        if (isDrawerOpen) viewModel.loadSessions()
    }

    if (showAbout) {
        AboutDialog(onDismiss = { showAbout = false })
    }

    ModalDrawerSheet {
        Column(modifier = Modifier.fillMaxWidth()) {
            Text(
                text = "对话历史",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(start = 16.dp, top = 16.dp, end = 16.dp, bottom = 8.dp),
            )

            HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

            if (uiState.isLoading) {
                Text(
                    "加载中...",
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodySmall,
                )
            } else if (uiState.sessions.isEmpty()) {
                Text(
                    "暂无对话",
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                LazyColumn(
                    modifier = Modifier.weight(1f),
                ) {
                    items(uiState.sessions, key = { it.id }) { session ->
                        SessionItem(
                            session = session,
                            isSelected = session.id == currentSessionId,
                            onSelect = { onSelectSession(session.id) },
                            onDelete = { viewModel.deleteSession(session.id) },
                            onRename = { name -> viewModel.renameSession(session.id, name) },
                        )
                    }
                }
            }

            HorizontalDivider()

            NavigationDrawerItem(
                label = { Text("设置") },
                selected = false,
                onClick = onSettings,
                icon = { Icon(Icons.Default.Settings, contentDescription = null) },
            )

            NavigationDrawerItem(
                label = { Text("关于") },
                selected = false,
                onClick = { showAbout = true },
                icon = { Icon(Icons.Default.Info, contentDescription = null) },
            )
        }
    }
}

@Composable
private fun SessionItem(
    session: ChatSession,
    isSelected: Boolean,
    onSelect: () -> Unit,
    onDelete: () -> Unit,
    onRename: (String) -> Unit,
) {
    var showRenameDialog by remember { mutableStateOf(false) }

    if (showRenameDialog) {
        RenameDialog(
            currentName = session.name ?: "",
            onConfirm = { onRename(it); showRenameDialog = false },
            onDismiss = { showRenameDialog = false },
        )
    }

    NavigationDrawerItem(
        label = {
            Text(
                text = session.name ?: session.id.take(8),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
        selected = isSelected,
        onClick = onSelect,
        badge = {
            IconButton(
                onClick = { showRenameDialog = true },
                modifier = Modifier.size(24.dp),
            ) {
                Icon(
                    Icons.Default.Edit,
                    contentDescription = "重命名",
                    modifier = Modifier.size(14.dp),
                )
            }
            IconButton(
                onClick = onDelete,
                modifier = Modifier.size(24.dp),
            ) {
                Icon(
                    Icons.Default.Delete,
                    contentDescription = "删除",
                    modifier = Modifier.size(14.dp),
                )
            }
        },
    )
}

@Composable
private fun RenameDialog(
    currentName: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf(currentName) }

    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("重命名对话") },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                singleLine = true,
            )
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(name) }) { Text("确定") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("取消") }
        },
    )
}

@Composable
private fun AboutDialog(onDismiss: () -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("关于") },
        text = {
            Column {
                Text("ClawSeed", style = MaterialTheme.typography.titleLarge)
                Spacer(modifier = Modifier.height(12.dp))
                Text("版本: ${BuildConfig.VERSION_NAME}", style = MaterialTheme.typography.bodyMedium)
                Spacer(modifier = Modifier.height(4.dp))
                Text("发布日期: ${BuildConfig.BUILD_DATE}", style = MaterialTheme.typography.bodyMedium)
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text("确定") }
        },
    )
}
