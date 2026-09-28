package dev.clawseed.demo.ui.drawer

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.ClickableText
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.clawseed.demo.BuildConfig
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.persona.PersonaDot
import dev.clawseed.demo.ui.settings.UpdateCheckResult
import dev.clawseed.demo.ui.settings.SettingsViewModel
import dev.clawseed.sdk.core.model.PersonaInfo
import dev.clawseed.sdk.core.model.SessionSummary
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private data class PersonaDeletionRequest(val persona: String?, val sessionIds: List<String>)

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
    var query by rememberSaveable { mutableStateOf("") }
    val history = remember(uiState.sessions, query, uiState.pinnedSessionIds) {
        buildSessionHistory(uiState.sessions, query, uiState.pinnedSessionIds)
    }
    val groups = history.groups
    var deletionRequest by remember { mutableStateOf<PersonaDeletionRequest?>(null) }
    val deleting = uiState.deletingSessionIds.isNotEmpty()
    val latestSessionId by rememberUpdatedState(currentSessionId)
    val latestDeleteCurrent by rememberUpdatedState(onDeleteCurrentSession)
    // Searching expands matches without changing the saved browsing preference.
    var searchCollapsedKeys by rememberSaveable(query) { mutableStateOf(emptyList<String>()) }
    val searching = query.isNotBlank()
    val today = remember(isDrawerOpen) { LocalDate.now() }
    val dateFormatter = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM)
    val focusManager = LocalFocusManager.current
    val keyboardController = LocalSoftwareKeyboardController.current

    fun dismissSearchInput() {
        focusManager.clearFocus(force = true)
        keyboardController?.hide()
    }

    LaunchedEffect(isDrawerOpen) {
        if (!isDrawerOpen) dismissSearchInput()
    }

    // Refresh session list every time drawer opens
    LaunchedEffect(isDrawerOpen, refreshKey) {
        if (isDrawerOpen) viewModel.loadSessions()
    }

    if (showAbout) {
        AboutDialog(onDismiss = { showAbout = false })
    }

    deletionRequest?.let { request ->
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { deletionRequest = null },
            title = { Text(stringResource(R.string.drawer_delete_persona_sessions)) },
            text = {
                Text(stringResource(
                    R.string.drawer_delete_persona_confirm,
                    request.persona ?: stringResource(R.string.drawer_default_persona),
                    request.sessionIds.size,
                ))
            },
            confirmButton = {
                TextButton(
                    enabled = !deleting,
                    onClick = {
                        deletionRequest = null
                        viewModel.deleteSessions(request.sessionIds) { id ->
                            if (id == latestSessionId) latestDeleteCurrent()
                        }
                    },
                ) { Text(stringResource(R.string.common_delete), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { deletionRequest = null }) { Text(stringResource(R.string.common_cancel)) }
            },
        )
    }

    val renderSession: @Composable (SessionSummary, Boolean) -> Unit = { session, pinned ->
        SessionItem(
            session = session,
            dateLabel = when (val date = sessionHistoryDate(session)) {
                today -> stringResource(R.string.drawer_today)
                today.minusDays(1) -> stringResource(R.string.drawer_yesterday)
                null -> stringResource(R.string.drawer_older)
                else -> date.format(dateFormatter)
            },
            personaLabel = if (pinned) sessionPersona(session)
                ?: stringResource(R.string.drawer_default_persona) else null,
            isSelected = session.id == currentSessionId,
            isPinned = pinned,
            actionsEnabled = !deleting,
            onSelect = { onSelectSession(session.id) },
            onDelete = {
                viewModel.deleteSession(session.id) {
                    if (session.id == latestSessionId) latestDeleteCurrent()
                }
            },
            onRename = { name -> viewModel.renameSession(session.id, name) },
            onTogglePinned = { viewModel.togglePinned(session.id) },
        )
    }

    ModalDrawerSheet {
        Column(modifier = Modifier.fillMaxWidth().imePadding()) {
            Text(
                text = stringResource(R.string.drawer_chat_history),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(start = 16.dp, top = 16.dp, end = 16.dp, bottom = 8.dp),
            )

            HorizontalDivider(modifier = Modifier.padding(vertical = 4.dp))
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                label = { Text(stringResource(R.string.drawer_search)) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { dismissSearchInput() }),
                leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
                trailingIcon = {
                    if (query.isNotEmpty()) IconButton(onClick = { query = "" }) {
                        Icon(Icons.Default.Close, contentDescription = stringResource(R.string.drawer_clear_search))
                    }
                },
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
            )
            if (uiState.error != null) {
                Column(Modifier.padding(horizontal = 16.dp)) {
                    Text(uiState.error!!, color = MaterialTheme.colorScheme.error, maxLines = 3)
                    TextButton(onClick = viewModel::loadSessions) { Text(stringResource(R.string.common_retry)) }
                }
            }
            if (deleting) {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                Text(
                    stringResource(R.string.drawer_deleting_sessions),
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            if (uiState.deletionError != null) {
                Column(Modifier.padding(horizontal = 16.dp)) {
                    Text(uiState.deletionError!!, color = MaterialTheme.colorScheme.error)
                    TextButton(onClick = viewModel::dismissDeletionError) { Text(stringResource(R.string.common_close)) }
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth()) {
                if (uiState.isLoading && uiState.sessions.isEmpty()) {
                    Text(
                        stringResource(R.string.common_loading),
                        modifier = Modifier.padding(16.dp),
                        style = MaterialTheme.typography.bodySmall,
                    )
                } else if (groups.isEmpty() && uiState.error == null) {
                    Text(
                        stringResource(if (query.isBlank()) R.string.drawer_no_conversations else R.string.drawer_no_matches),
                        modifier = Modifier.padding(16.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    LazyColumn(modifier = Modifier.fillMaxWidth()) {
                        if (history.pinned.isNotEmpty()) {
                            item(key = "pinned-header", contentType = "pinned-header") {
                                Text(
                                    text = stringResource(R.string.drawer_pinned_section),
                                    style = MaterialTheme.typography.titleSmall,
                                    color = MaterialTheme.colorScheme.primary,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                                )
                            }
                            items(history.pinned, key = { "session:${it.id}" }, contentType = { "session" }) {
                                renderSession(it, true)
                            }
                        }
                        groups.forEach { group ->
                            val groupKey = group.key
                            val expanded = if (searching) groupKey !in searchCollapsedKeys
                            else groupKey !in uiState.collapsedPersonaKeys
                            item(key = "persona:$groupKey", contentType = "persona") {
                                PersonaGroupHeader(
                                    persona = group.persona,
                                    personaVisuals = uiState.personaVisuals,
                                    expanded = expanded,
                                    sessionCount = group.sessions.size + group.pinnedCount,
                                    actionsEnabled = !deleting,
                                    onDeleteAll = {
                                        deletionRequest = PersonaDeletionRequest(
                                            group.persona, personaSessionIds(uiState.sessions, group.persona),
                                        )
                                    },
                                    onClick = {
                                        if (searching) {
                                            searchCollapsedKeys = if (expanded) searchCollapsedKeys + groupKey
                                            else searchCollapsedKeys - groupKey
                                        } else {
                                            viewModel.togglePersonaCollapsed(groupKey)
                                        }
                                    },
                                )
                            }
                            if (expanded) {
                                if (group.sessions.isEmpty() && group.pinnedCount > 0) {
                                    item(key = "pinned-only:$groupKey", contentType = "hint") {
                                        Text(
                                            stringResource(R.string.drawer_persona_all_pinned),
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                            modifier = Modifier.padding(start = 24.dp, end = 16.dp, bottom = 8.dp),
                                        )
                                    }
                                }
                                items(group.sessions, key = { "session:${it.id}" }, contentType = { "session" }) { session ->
                                    renderSession(session, false)
                                }
                            }
                        }
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
private fun PersonaGroupHeader(
    persona: String?,
    personaVisuals: Map<String, PersonaInfo>,
    expanded: Boolean,
    sessionCount: Int,
    actionsEnabled: Boolean,
    onDeleteAll: () -> Unit,
    onClick: () -> Unit,
) {
    var showMenu by remember { mutableStateOf(false) }
    val visual = persona?.let(personaVisuals::get)
    val actionLabel = stringResource(if (expanded) R.string.drawer_collapse_persona else R.string.drawer_expand_persona)
    val expandedLabel = stringResource(if (expanded) R.string.drawer_group_expanded else R.string.drawer_group_collapsed)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .semantics { stateDescription = expandedLabel }
            .clickable(onClickLabel = actionLabel, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (persona != null) {
            PersonaDot(
                persona,
                Modifier.size(24.dp),
                showInitial = true,
                avatar = visual?.avatar,
                color = visual?.color,
            )
            Spacer(Modifier.width(8.dp))
        }
        Text(
            text = persona ?: stringResource(R.string.drawer_default_persona),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            text = sessionCount.toString(),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 8.dp),
        )
        Icon(
            imageVector = if (expanded) Icons.Default.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Box {
            IconButton(enabled = actionsEnabled, onClick = { showMenu = true }) {
                Icon(Icons.Default.MoreVert, contentDescription = stringResource(R.string.drawer_persona_actions))
            }
            DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
                DropdownMenuItem(
                    enabled = actionsEnabled,
                    text = { Text(stringResource(R.string.drawer_delete_persona_sessions), color = MaterialTheme.colorScheme.error) },
                    leadingIcon = { Icon(Icons.Default.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
                    onClick = { showMenu = false; onDeleteAll() },
                )
            }
        }
    }
}

@Composable
private fun SessionItem(
    session: SessionSummary,
    dateLabel: String,
    personaLabel: String?,
    actionsEnabled: Boolean,
    isSelected: Boolean,
    isPinned: Boolean,
    onSelect: () -> Unit,
    onDelete: () -> Unit,
    onRename: (String) -> Unit,
    onTogglePinned: () -> Unit,
) {
    var showRenameDialog by remember { mutableStateOf(false) }
    var showMenu by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    if (confirmDelete) {
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text(stringResource(R.string.common_delete), color = MaterialTheme.colorScheme.error) },
            text = { Text(stringResource(R.string.drawer_delete_confirm, session.name ?: session.id.take(8))) },
            confirmButton = {
                TextButton(enabled = actionsEnabled, onClick = { confirmDelete = false; onDelete() }) { Text(stringResource(R.string.common_delete), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = false }) { Text(stringResource(R.string.common_cancel)) }
            },
        )
    }

    if (showRenameDialog) {
        RenameDialog(
            currentName = session.name ?: "",
            onConfirm = { onRename(it); showRenameDialog = false },
            onDismiss = { showRenameDialog = false },
        )
    }

    NavigationDrawerItem(
        label = {
            Column {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = session.name ?: session.id.take(8),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (isPinned) {
                        Spacer(Modifier.width(4.dp))
                        Icon(
                            Icons.Default.Star,
                            contentDescription = stringResource(R.string.drawer_pinned),
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(16.dp),
                        )
                    }
                }
                if (personaLabel != null) {
                    Text(
                        text = personaLabel,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Text(
                    text = dateLabel,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 3.dp),
                )
            }
        },
        selected = isSelected,
        onClick = { if (actionsEnabled) onSelect() },
        badge = {
            Box {
                IconButton(enabled = actionsEnabled, onClick = { showMenu = true }) {
                    Icon(Icons.Default.MoreVert, contentDescription = stringResource(R.string.drawer_session_actions))
                }
                DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
                    DropdownMenuItem(
                        enabled = actionsEnabled,
                        text = { Text(stringResource(if (isPinned) R.string.drawer_unpin else R.string.drawer_pin)) },
                        leadingIcon = { Icon(Icons.Default.Star, contentDescription = null) },
                        onClick = { showMenu = false; onTogglePinned() },
                    )
                    DropdownMenuItem(
                        enabled = actionsEnabled,
                        text = { Text(stringResource(R.string.drawer_rename)) },
                        leadingIcon = { Icon(Icons.Default.Edit, contentDescription = null) },
                        onClick = { showMenu = false; showRenameDialog = true },
                    )
                    DropdownMenuItem(
                        enabled = actionsEnabled,
                        text = { Text(stringResource(R.string.common_delete), color = MaterialTheme.colorScheme.error) },
                        leadingIcon = { Icon(Icons.Default.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
                        onClick = { showMenu = false; confirmDelete = true },
                    )
                }
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
