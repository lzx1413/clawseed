package dev.clawseed.demo.ui.drawer

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.ClickableText
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.BuildConfig
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.persona.PersonaDot
import dev.clawseed.demo.ui.persona.personaContainerColor
import dev.clawseed.demo.ui.persona.personaContentColor
import dev.clawseed.demo.ui.settings.UpdateCheckResult
import dev.clawseed.demo.ui.settings.SettingsViewModel
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.SessionSummary

@Composable
fun SessionDrawer(
    currentSessionId: String?,
    onSelectSession: (String) -> Unit,
    onDeleteCurrentSession: () -> Unit = {},
    onSettings: () -> Unit,
    onScheduledTasks: () -> Unit = {},
    onPersonas: () -> Unit = {},
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
                text = stringResource(R.string.drawer_chat_history),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(start = 16.dp, top = 16.dp, end = 16.dp, bottom = 8.dp),
            )

            HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))

            if (uiState.isLoading) {
                Text(
                    stringResource(R.string.common_loading),
                    modifier = Modifier.padding(16.dp),
                    style = MaterialTheme.typography.bodySmall,
                )
            } else if (uiState.sessions.isEmpty()) {
                Text(
                    stringResource(R.string.drawer_no_conversations),
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
                            personaVisuals = uiState.personaVisuals,
                            isSelected = session.id == currentSessionId,
                            onSelect = { onSelectSession(session.id) },
                            onDelete = {
                                viewModel.deleteSession(session.id) {
                                    if (session.id == currentSessionId) {
                                        onDeleteCurrentSession()
                                    }
                                }
                            },
                            onRename = { name -> viewModel.renameSession(session.id, name) },
                        )
                    }
                }
            }

            HorizontalDivider()

            NavigationDrawerItem(
                label = { Text(stringResource(R.string.drawer_personas)) },
                selected = false,
                onClick = onPersonas,
                icon = { Icon(Icons.Default.Person, contentDescription = null) },
            )

            NavigationDrawerItem(
                label = { Text(stringResource(R.string.drawer_scheduled_tasks)) },
                selected = false,
                onClick = onScheduledTasks,
                icon = { Icon(Icons.Default.Info, contentDescription = null) },
            )

            NavigationDrawerItem(
                label = { Text(stringResource(R.string.drawer_settings)) },
                selected = false,
                onClick = onSettings,
                icon = { Icon(Icons.Default.Settings, contentDescription = null) },
            )

            NavigationDrawerItem(
                label = { Text(stringResource(R.string.drawer_about)) },
                selected = false,
                onClick = { showAbout = true },
                icon = { Icon(Icons.Default.Info, contentDescription = null) },
            )
        }
    }
}

@Composable
private fun SessionItem(
    session: SessionSummary,
    personaVisuals: Map<String, PersonaInfo>,
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
            val persona = session.persona
            val personaVisual = persona?.let { personaVisuals[it] }
            Column {
                Text(
                    text = session.name ?: session.id.take(8),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (!persona.isNullOrEmpty()) {
                    Row(
                        modifier = Modifier
                            .padding(top = 3.dp)
                            .clip(RoundedCornerShape(999.dp))
                            .background(personaContainerColor(persona, personaVisual?.color))
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        PersonaDot(
                            persona,
                            Modifier.size(14.dp),
                            showInitial = true,
                            avatar = personaVisual?.avatar,
                            color = personaVisual?.color,
                        )
                        Spacer(Modifier.width(5.dp))
                        Text(
                            text = persona,
                            style = MaterialTheme.typography.labelSmall,
                            color = personaContentColor(persona, personaVisual?.color),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
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
                    contentDescription = stringResource(R.string.drawer_rename),
                    modifier = Modifier.size(14.dp),
                )
            }
            IconButton(
                onClick = onDelete,
                modifier = Modifier.size(24.dp),
            ) {
                Icon(
                    Icons.Default.Delete,
                    contentDescription = stringResource(R.string.common_delete),
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
        title = { Text(stringResource(R.string.drawer_rename_dialog_title)) },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                singleLine = true,
            )
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(name) }) { Text(stringResource(R.string.common_confirm)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.common_cancel)) }
        },
    )
}

@Composable
private fun AboutDialog(onDismiss: () -> Unit) {
    val viewModel: SettingsViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val uriHandler = LocalUriHandler.current
    val context = LocalContext.current
    val githubUrl = "https://github.com/lzx1413/clawseed"
    val annotatedLink = buildAnnotatedString {
        pushStringAnnotation(tag = "URL", annotation = githubUrl)
        withStyle(SpanStyle(color = MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline)) {
            append(githubUrl)
        }
        pop()
    }

    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.drawer_about_title)) },
        text = {
            Column {
                Text("ClawSeed", style = MaterialTheme.typography.titleLarge)
                Spacer(modifier = Modifier.height(12.dp))
                Text(stringResource(R.string.drawer_version, BuildConfig.VERSION_NAME), style = MaterialTheme.typography.bodyMedium)
                Spacer(modifier = Modifier.height(4.dp))
                Text(stringResource(R.string.drawer_build_date, BuildConfig.BUILD_DATE), style = MaterialTheme.typography.bodyMedium)
                Spacer(modifier = Modifier.height(4.dp))
                Text("Agent SDK: ${BuildConfig.SDK_VERSION}", style = MaterialTheme.typography.bodyMedium)
                Spacer(modifier = Modifier.height(8.dp))
                ClickableText(
                    text = annotatedLink,
                    style = MaterialTheme.typography.bodyMedium,
                    onClick = { offset ->
                        annotatedLink.getStringAnnotations(tag = "URL", start = offset, end = offset)
                            .firstOrNull()?.let { uriHandler.openUri(it.item) }
                    },
                )

                // ── Update section ──
                Spacer(modifier = Modifier.height(16.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(stringResource(R.string.update_check), style = MaterialTheme.typography.titleSmall)
                    if (uiState.isCheckingUpdate) {
                        androidx.compose.material3.CircularProgressIndicator(
                            modifier = Modifier.size(18.dp),
                            strokeWidth = 2.dp,
                        )
                    } else {
                        OutlinedButton(onClick = { viewModel.checkForUpdate() }) {
                            Icon(
                                Icons.Default.Refresh,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                            )
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(stringResource(R.string.update_check))
                        }
                    }
                }

                // Update check result
                when (val result = uiState.updateCheckResult) {
                    is UpdateCheckResult.UpToDate -> {
                        Spacer(modifier = Modifier.height(8.dp))
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(
                                Icons.Default.Check,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.size(18.dp),
                            )
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(
                                stringResource(R.string.update_up_to_date),
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.primary,
                            )
                        }
                    }
                    is UpdateCheckResult.Error -> {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            stringResource(R.string.update_check_failed, result.message),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                    null -> {}
                }

                // Update available
                val updateInfo = uiState.updateInfo
                if (updateInfo != null) {
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(
                        stringResource(R.string.update_available, updateInfo.versionName),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.primary,
                    )

                    // Release notes (collapsible)
                    if (updateInfo.releaseNotes.isNotBlank()) {
                        Spacer(modifier = Modifier.height(8.dp))
                        var notesExpanded by remember { mutableStateOf(false) }
                        Column {
                            Text(
                                text = stringResource(R.string.update_release_notes),
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.clickable { notesExpanded = !notesExpanded },
                            )
                            androidx.compose.animation.AnimatedVisibility(visible = notesExpanded) {
                                Text(
                                    text = updateInfo.releaseNotes,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    modifier = Modifier.padding(top = 4.dp),
                                )
                            }
                        }
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    // Download progress
                    val progress = uiState.updateDownloadProgress
                    if (uiState.isDownloadingUpdate && progress != null) {
                        LinearProgressIndicator(
                            progress = { progress.percent.toFloat() / 100f },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Spacer(modifier = Modifier.height(4.dp))
                        Text(
                            text = stringResource(
                                R.string.update_download_progress,
                                progress.percent,
                                formatBytes(progress.downloadedBytes),
                                formatBytes(progress.totalBytes),
                            ),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    } else if (uiState.updateApkReady) {
                        // APK downloaded and ready to install
                        Button(
                            onClick = {
                                if (dev.clawseed.demo.updater.ApkInstaller.canInstallPackages(context)) {
                                    viewModel.installUpdate()
                                } else {
                                    context.startActivity(viewModel.getInstallPermissionIntent())
                                }
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Icon(Icons.Default.Check, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(stringResource(R.string.update_install))
                        }
                    } else if (!uiState.isDownloadingUpdate) {
                        // Download button
                        Button(
                            onClick = { viewModel.downloadUpdate() },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Icon(Icons.Default.Refresh, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(modifier = Modifier.width(6.dp))
                            Text(stringResource(R.string.update_download))
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.common_confirm)) }
        },
    )
}

private fun formatBytes(bytes: Long): String = when {
    bytes >= 1_000_000_000 -> "${bytes / 1_000_000_000} GB"
    bytes >= 1_000_000 -> "%.1f MB".format(bytes / 1_000_000.0)
    bytes >= 1_000 -> "${bytes / 1_000} KB"
    else -> "$bytes B"
}
