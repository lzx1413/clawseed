package dev.clawseed.demo.ui.settings

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.TextButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.datatransfer.DataCategory
import dev.clawseed.demo.datatransfer.DataTransferManager
import dev.clawseed.demo.datatransfer.ImportResult
import dev.clawseed.demo.datatransfer.ImportStrategy
import dev.clawseed.demo.datatransfer.WebDavSyncClient
import dev.clawseed.demo.datatransfer.WebDavSettings
import dev.clawseed.demo.datatransfer.WebDavSettingsStore
import dev.clawseed.demo.i18n.desc
import dev.clawseed.demo.i18n.label
import dev.clawseed.demo.ui.chat.RichMediaCache
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable
fun DataTransferSection() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val manager = remember { DataTransferManager(context) }
    val webDavSettingsStore = remember { WebDavSettingsStore(context) }

    // Export state
    var selectedExportCategories by remember { mutableStateOf(emptySet<DataCategory>()) }
    var excludeSensitive by remember { mutableStateOf(true) }
    var isExporting by remember { mutableStateOf(false) }
    var exportError by remember { mutableStateOf<String?>(null) }
    var exportSuccess by remember { mutableStateOf<String?>(null) }

    // WebDAV sync state. The optional WebDAV password is persisted only through Keystore.
    var webDavUrl by remember { mutableStateOf("") }
    var webDavDirectory by remember { mutableStateOf("clawseed") }
    var webDavLastFile by remember { mutableStateOf("") }
    var webDavUsername by remember { mutableStateOf("") }
    var webDavPassword by remember { mutableStateOf("") }
    var webDavEncryptionPassword by remember { mutableStateOf("") }
    var rememberWebDavPassword by remember { mutableStateOf(false) }
    var webDavSettingsLoaded by remember { mutableStateOf(false) }
    var webDavBusy by remember { mutableStateOf(false) }
    var webDavStatus by remember { mutableStateOf<String?>(null) }
    var webDavBackups by remember { mutableStateOf(emptyList<WebDavSyncClient.RemoteBackup>()) }
    var webDavBackupsLoading by remember { mutableStateOf(false) }
    var webDavListError by remember { mutableStateOf<String?>(null) }
    var webDavDeleteCandidate by remember { mutableStateOf<WebDavSyncClient.RemoteBackup?>(null) }
    var exportDestination by remember { mutableStateOf(TransferEndpoint.LocalFile) }
    var importSource by remember { mutableStateOf(TransferEndpoint.LocalFile) }
    var webDavSelectedFile by remember { mutableStateOf("") }

    LaunchedEffect(Unit) {
        val settings = withContext(Dispatchers.IO) { webDavSettingsStore.load() }
        webDavUrl = settings.url
        webDavDirectory = settings.remoteDirectory
        webDavLastFile = settings.lastRemoteFile
        webDavSelectedFile = settings.lastRemoteFile
        webDavUsername = settings.username
        webDavPassword = settings.password
        rememberWebDavPassword = settings.rememberPassword
        webDavSettingsLoaded = true
    }

    suspend fun saveWebDavSettings(lastRemoteFile: String = webDavLastFile) {
        val normalizedDirectory = WebDavSettingsStore.normalizeDirectory(webDavDirectory)
        webDavDirectory = normalizedDirectory
        withContext(Dispatchers.IO) {
            webDavSettingsStore.save(
                WebDavSettings(
                    url = webDavUrl,
                    remoteDirectory = normalizedDirectory,
                    lastRemoteFile = lastRemoteFile,
                    username = webDavUsername,
                    password = webDavPassword,
                    rememberPassword = rememberWebDavPassword,
                ),
            )
        }
    }

    suspend fun refreshWebDavBackups() {
        if (webDavUrl.isBlank() || webDavDirectory.isBlank()) return
        webDavBackupsLoading = true
        webDavListError = null
        try {
            val result = manager.listWebDavBackups(
                webDavUrl,
                WebDavSettingsStore.normalizeDirectory(webDavDirectory),
                webDavUsername.takeIf(String::isNotBlank),
                webDavPassword.takeIf(String::isNotBlank),
            )
            if (result.isSuccess) {
                val backups = result.getOrThrow()
                webDavBackups = backups
                webDavSelectedFile = webDavSelectedFile.takeIf { selected ->
                    backups.any { it.path == selected }
                } ?: webDavLastFile.takeIf { selected ->
                    backups.any { it.path == selected }
                } ?: backups.firstOrNull()?.path.orEmpty()
            } else {
                webDavListError = result.exceptionOrNull()?.message ?: ""
            }
        } finally {
            webDavBackupsLoading = false
        }
    }

    LaunchedEffect(importSource, webDavSettingsLoaded) {
        if (importSource == TransferEndpoint.WebDav && webDavSettingsLoaded) {
            refreshWebDavBackups()
        }
    }

    // Import state
    var selectedImportCategories by remember { mutableStateOf(emptySet<DataCategory>()) }
    var importStrategies by remember { mutableStateOf(mapOf<DataCategory, ImportStrategy>()) }
    var isImporting by remember { mutableStateOf(false) }
    var importResult by remember { mutableStateOf<ImportResult?>(null) }
    var importError by remember { mutableStateOf<String?>(null) }

    // Rich media is deliberately separated from exported conversation data.
    var mediaCacheBytes by remember { mutableStateOf(RichMediaCache.sizeBytes(context)) }
    var isClearingMediaCache by remember { mutableStateOf(false) }
    var mediaCacheStatus by remember { mutableStateOf<String?>(null) }

    // Pre-resolve format strings for use inside coroutines (non-Composable context)
    val exportSuccessFormat = stringResource(R.string.data_export_success)
    val exportFailedFormat = stringResource(R.string.data_export_failed)
    val importFailedFormat = stringResource(R.string.data_import_failed)
    val cannotReadFile = stringResource(R.string.data_cannot_read_file)
    val mediaCacheCleared = stringResource(R.string.data_media_cache_cleared)
    val mediaCacheClearFailed = stringResource(R.string.data_media_cache_clear_failed)
    val webDavBackupDoneFormat = stringResource(R.string.data_webdav_backup_done)
    val webDavRestoreDone = stringResource(R.string.data_webdav_restore_done)
    val webDavFailedFormat = stringResource(R.string.data_webdav_failed)
    val webDavSettingsSaved = stringResource(R.string.data_webdav_settings_saved)
    val webDavDeleteDone = stringResource(R.string.data_webdav_delete_done)

    // SAF launchers
    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/zip"),
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            isExporting = true
            exportError = null
            exportSuccess = null
            try {
                withContext(Dispatchers.IO) {
                    context.contentResolver.openOutputStream(uri)?.use { outputStream ->
                        manager.exportData(selectedExportCategories, excludeSensitive, outputStream)
                            .getOrElse { throw it }
                    }
                }
                val categoryNames = selectedExportCategories.map { it.label(context) }.joinToString(", ")
                exportSuccess = exportSuccessFormat.format(categoryNames)
            } catch (e: Exception) {
                exportError = exportFailedFormat.format(e.message ?: "")
            } finally {
                isExporting = false
            }
        }
    }

    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            isImporting = true
            importError = null
            importResult = null
            try {
                val result = withContext(Dispatchers.IO) {
                    context.contentResolver.openInputStream(uri)?.use { inputStream ->
                        manager.importData(inputStream, selectedImportCategories, importStrategies)
                            .getOrElse { throw it }
                    } ?: throw IllegalStateException(cannotReadFile)
                }
                importResult = result
            } catch (e: Exception) {
                importError = importFailedFormat.format(e.message ?: "")
            } finally {
                isImporting = false
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.data_media_cache), style = MaterialTheme.typography.titleMedium)
                Text(
                    stringResource(R.string.data_media_cache_desc),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    stringResource(R.string.data_media_cache_size, formatByteSize(mediaCacheBytes)),
                    style = MaterialTheme.typography.labelMedium,
                )
                Button(
                    onClick = {
                        scope.launch {
                            isClearingMediaCache = true
                            mediaCacheStatus = null
                            val cleared = withContext(Dispatchers.IO) { RichMediaCache.clear(context) }
                            mediaCacheBytes = withContext(Dispatchers.IO) { RichMediaCache.sizeBytes(context) }
                            mediaCacheStatus = if (cleared) mediaCacheCleared else mediaCacheClearFailed
                            isClearingMediaCache = false
                        }
                    },
                    enabled = !isClearingMediaCache,
                ) {
                    if (isClearingMediaCache) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                        Spacer(Modifier.width(8.dp))
                    }
                    Text(stringResource(R.string.data_clear_media_cache))
                }
                mediaCacheStatus?.let { status ->
                    Text(status, style = MaterialTheme.typography.bodySmall)
                }
            }
        }

        // Export card
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.data_export), style = MaterialTheme.typography.titleMedium)

                // Category checkboxes
                for (category in DataCategory.entries) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Checkbox(
                            checked = category in selectedExportCategories,
                            onCheckedChange = { checked ->
                                selectedExportCategories = if (checked) {
                                    selectedExportCategories + category
                                } else {
                                    selectedExportCategories - category
                                }
                            },
                        )
                        Spacer(modifier = Modifier.width(4.dp))
                        Column {
                            Text(category.label(), style = MaterialTheme.typography.bodyMedium)
                            Text(
                                category.desc(),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }

                // Exclude sensitive switch
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Switch(
                        checked = excludeSensitive,
                        onCheckedChange = { excludeSensitive = it },
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Column {
                        Text(stringResource(R.string.data_exclude_sensitive), style = MaterialTheme.typography.bodyMedium)
                        Text(
                            stringResource(R.string.data_exclude_sensitive_desc),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }

                Text(stringResource(R.string.data_transfer_destination), style = MaterialTheme.typography.labelLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(
                        selected = exportDestination == TransferEndpoint.LocalFile,
                        onClick = { exportDestination = TransferEndpoint.LocalFile },
                        label = { Text(stringResource(R.string.data_transfer_local_file)) },
                    )
                    FilterChip(
                        selected = exportDestination == TransferEndpoint.WebDav,
                        onClick = { exportDestination = TransferEndpoint.WebDav },
                        label = { Text(stringResource(R.string.data_transfer_webdav)) },
                    )
                }

                // Export button
                Button(
                    onClick = {
                        val fileName = exportFileName(selectedExportCategories)
                        if (exportDestination == TransferEndpoint.LocalFile) {
                            exportLauncher.launch(fileName)
                        } else {
                            scope.launch {
                                webDavBusy = true
                                webDavStatus = null
                                try {
                                    val remoteFile = remoteBackupPath(webDavDirectory, fileName)
                                    val result = manager.backupToWebDav(
                                        selectedExportCategories,
                                        excludeSensitive,
                                        webDavUrl,
                                        remoteFile,
                                        webDavUsername.takeIf(String::isNotBlank),
                                        webDavPassword.takeIf(String::isNotBlank),
                                        webDavEncryptionPassword,
                                    )
                                    if (result.isSuccess) {
                                        val upload = result.getOrThrow()
                                        webDavLastFile = remoteFile
                                        webDavSelectedFile = remoteFile
                                        saveWebDavSettings(remoteFile)
                                        selectedImportCategories = if (excludeSensitive) {
                                            selectedExportCategories - setOf(
                                                DataCategory.MEMORY,
                                                DataCategory.USER_PROFILE,
                                            )
                                        } else {
                                            selectedExportCategories
                                        }
                                        webDavStatus = webDavBackupDoneFormat.format(upload.bytes)
                                    } else {
                                        webDavStatus = webDavFailedFormat.format(
                                            result.exceptionOrNull()?.message ?: "",
                                        )
                                    }
                                } finally {
                                    webDavBusy = false
                                }
                            }
                        }
                    },
                    enabled = selectedExportCategories.isNotEmpty() && !isExporting && !webDavBusy &&
                        (exportDestination == TransferEndpoint.LocalFile || webDavSettingsLoaded),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (isExporting || webDavBusy) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(if (exportDestination == TransferEndpoint.WebDav) stringResource(R.string.data_webdav_uploading) else stringResource(R.string.data_exporting))
                    } else {
                        Text(if (exportDestination == TransferEndpoint.WebDav) stringResource(R.string.data_webdav_backup) else stringResource(R.string.data_export_btn))
                    }
                }

                // Status messages
                exportSuccess?.let {
                    Text(it, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodyMedium)
                }
                exportError?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
                }
                if (exportDestination == TransferEndpoint.WebDav) {
                    webDavStatus?.let {
                        Text(it, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
        }

        // WebDAV card
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.data_webdav_title), style = MaterialTheme.typography.titleMedium)
                Text(
                    stringResource(R.string.data_webdav_desc),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                TextField(
                    value = webDavUrl,
                    onValueChange = { webDavUrl = it },
                    label = { Text(stringResource(R.string.data_webdav_url)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                TextField(
                    value = webDavDirectory,
                    onValueChange = { webDavDirectory = it },
                    label = { Text(stringResource(R.string.data_webdav_directory)) },
                    supportingText = { Text(stringResource(R.string.data_webdav_directory_hint)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                TextField(
                    value = webDavUsername,
                    onValueChange = { webDavUsername = it },
                    label = { Text(stringResource(R.string.data_webdav_username)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                TextField(
                    value = webDavPassword,
                    onValueChange = { webDavPassword = it },
                    label = { Text(stringResource(R.string.data_webdav_password)) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.fillMaxWidth(),
                )
                TextField(
                    value = webDavEncryptionPassword,
                    onValueChange = { webDavEncryptionPassword = it },
                    label = { Text(stringResource(R.string.data_webdav_encryption_password)) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Switch(
                        checked = rememberWebDavPassword,
                        onCheckedChange = { rememberWebDavPassword = it },
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(stringResource(R.string.data_webdav_remember_password))
                }
                Button(
                    onClick = {
                        scope.launch {
                            saveWebDavSettings()
                            webDavStatus = webDavSettingsSaved
                        }
                    },
                    enabled = webDavSettingsLoaded && !webDavBusy,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(R.string.data_webdav_save_settings)) }
                Text(
                    stringResource(R.string.data_webdav_selection_hint),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                webDavStatus?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                if (webDavLastFile.isNotBlank()) {
                    Text(
                        stringResource(R.string.data_webdav_last_file, webDavLastFile),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        // Import card
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.data_import), style = MaterialTheme.typography.titleMedium)

                Text(stringResource(R.string.data_transfer_source), style = MaterialTheme.typography.labelLarge)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(
                        selected = importSource == TransferEndpoint.LocalFile,
                        onClick = { importSource = TransferEndpoint.LocalFile },
                        label = { Text(stringResource(R.string.data_transfer_local_file)) },
                    )
                    FilterChip(
                        selected = importSource == TransferEndpoint.WebDav,
                        onClick = { importSource = TransferEndpoint.WebDav },
                        label = { Text(stringResource(R.string.data_transfer_webdav)) },
                    )
                }

                if (importSource == TransferEndpoint.WebDav) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(
                            stringResource(R.string.data_webdav_backups_title),
                            style = MaterialTheme.typography.labelLarge,
                        )
                        Button(
                            onClick = { scope.launch { refreshWebDavBackups() } },
                            enabled = webDavSettingsLoaded && !webDavBackupsLoading && !webDavBusy,
                        ) {
                            if (webDavBackupsLoading) {
                                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                                Spacer(Modifier.width(6.dp))
                            }
                            Text(stringResource(R.string.data_webdav_refresh_backups))
                        }
                    }
                    webDavListError?.let { error ->
                        Text(
                            stringResource(R.string.data_webdav_list_failed, error),
                            color = MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    if (!webDavBackupsLoading && webDavListError == null && webDavBackups.isEmpty()) {
                        Text(
                            stringResource(R.string.data_webdav_no_backups),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    for (backup in webDavBackups) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { webDavSelectedFile = backup.path },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(
                                selected = webDavSelectedFile == backup.path,
                                onClick = { webDavSelectedFile = backup.path },
                            )
                            Column(
                                modifier = Modifier
                                    .weight(1f)
                                    .padding(start = 4.dp),
                            ) {
                                Text(backup.name, style = MaterialTheme.typography.bodyMedium)
                                Text(
                                    stringResource(
                                        R.string.data_webdav_backup_details,
                                        formatByteSize(backup.bytes),
                                        backup.lastModified ?: "—",
                                    ),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            TextButton(
                                onClick = { webDavDeleteCandidate = backup },
                                enabled = !webDavBusy && !webDavBackupsLoading,
                            ) {
                                Text(stringResource(R.string.data_webdav_delete_backup))
                            }
                        }
                    }
                }

                // Category checkboxes
                for (category in DataCategory.entries) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Checkbox(
                            checked = category in selectedImportCategories,
                            onCheckedChange = { checked ->
                                selectedImportCategories = if (checked) {
                                    selectedImportCategories + category
                                } else {
                                    selectedImportCategories - category
                                }
                            },
                        )
                        Spacer(modifier = Modifier.width(4.dp))
                        Column {
                            Text(category.label(), style = MaterialTheme.typography.bodyMedium)
                            Text(
                                category.desc(),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    // Strategy selector for categories that support multiple strategies
                    if (category in selectedImportCategories && category in STRATEGY_CATEGORIES) {
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(start = 48.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            for (strategy in category.defaultStrategies) {
                                val currentStrategy = importStrategies[category] ?: category.defaultStrategy
                                FilterChip(
                                    selected = currentStrategy == strategy,
                                    onClick = {
                                        importStrategies = importStrategies.toMutableMap().apply {
                                            this[category] = strategy
                                        }
                                    },
                                    label = { Text(strategy.label()) },
                                )
                            }
                        }
                    }
                }

                // Warning text
                Text(
                    stringResource(R.string.data_import_warning),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )

                // Import button
                Button(
                    onClick = {
                        if (importSource == TransferEndpoint.LocalFile) {
                            importLauncher.launch(arrayOf("application/zip", "*/*"))
                        } else {
                            scope.launch {
                                webDavBusy = true
                                webDavStatus = null
                                importError = null
                                importResult = null
                                try {
                                    val result = manager.restoreFromWebDav(
                                        selectedImportCategories,
                                        importStrategies,
                                        webDavUrl,
                                        webDavSelectedFile,
                                        webDavUsername.takeIf(String::isNotBlank),
                                        webDavPassword.takeIf(String::isNotBlank),
                                        webDavEncryptionPassword,
                                    )
                                    if (result.isSuccess) {
                                        importResult = result.getOrThrow()
                                        webDavStatus = webDavRestoreDone
                                    } else {
                                        webDavStatus = webDavFailedFormat.format(
                                            result.exceptionOrNull()?.message ?: "",
                                        )
                                    }
                                } finally {
                                    webDavBusy = false
                                }
                            }
                        }
                    },
                    enabled = selectedImportCategories.isNotEmpty() && !isImporting && !webDavBusy &&
                        (importSource == TransferEndpoint.LocalFile ||
                            (webDavSettingsLoaded && webDavSelectedFile.isNotBlank())),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (isImporting || webDavBusy) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            if (importSource == TransferEndpoint.WebDav) {
                                stringResource(R.string.data_webdav_downloading)
                            } else {
                                stringResource(R.string.data_importing)
                            },
                        )
                    } else {
                        Text(
                            if (importSource == TransferEndpoint.WebDav) {
                                stringResource(R.string.data_webdav_restore)
                            } else {
                                stringResource(R.string.data_import_btn)
                            },
                        )
                    }
                }

                // Status messages
                importError?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }

    // Import result dialog
    importResult?.let { result ->
        AlertDialog(
            onDismissRequest = { importResult = null },
            title = {
                Text(
                    if (result.isSuccess) stringResource(R.string.data_import_complete)
                    else stringResource(R.string.data_import_issues),
                )
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(result.summaryString(), style = MaterialTheme.typography.bodyMedium)
                    if (result.warnings.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(stringResource(R.string.data_warning_label), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        for (w in result.warnings) {
                            Text(w, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    if (result.errors.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(stringResource(R.string.data_error_label), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                        for (e in result.errors) {
                            Text(e, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                        }
                    }
                }
            },
            confirmButton = {
                Button(onClick = { importResult = null }) { Text(stringResource(R.string.common_confirm)) }
            },
        )
    }

    webDavDeleteCandidate?.let { backup ->
        AlertDialog(
            onDismissRequest = { if (!webDavBusy) webDavDeleteCandidate = null },
            title = { Text(stringResource(R.string.data_webdav_delete_title)) },
            text = { Text(stringResource(R.string.data_webdav_delete_message, backup.name)) },
            dismissButton = {
                TextButton(
                    onClick = { webDavDeleteCandidate = null },
                    enabled = !webDavBusy,
                ) { Text(stringResource(R.string.common_cancel)) }
            },
            confirmButton = {
                Button(
                    onClick = {
                        scope.launch {
                            webDavBusy = true
                            webDavStatus = null
                            try {
                                val result = manager.deleteWebDavBackup(
                                    webDavUrl,
                                    backup.path,
                                    webDavUsername.takeIf(String::isNotBlank),
                                    webDavPassword.takeIf(String::isNotBlank),
                                )
                                if (result.isSuccess) {
                                    if (webDavLastFile == backup.path) {
                                        webDavLastFile = ""
                                        webDavSelectedFile = ""
                                        saveWebDavSettings("")
                                    }
                                    webDavDeleteCandidate = null
                                    webDavStatus = webDavDeleteDone
                                    refreshWebDavBackups()
                                } else {
                                    webDavStatus = webDavFailedFormat.format(
                                        result.exceptionOrNull()?.message ?: "",
                                    )
                                }
                            } finally {
                                webDavBusy = false
                            }
                        }
                    },
                    enabled = !webDavBusy,
                ) { Text(stringResource(R.string.data_webdav_delete_backup)) }
            },
        )
    }
}

private enum class TransferEndpoint {
    LocalFile,
    WebDav,
}

private fun exportFileName(categories: Set<DataCategory>): String {
    val date = SimpleDateFormat("yyyyMMdd", Locale.getDefault()).format(Date())
    val names = categories.joinToString("-") { it.name.lowercase() }
    return "clawseed-$date-$names.zip"
}

private fun remoteBackupPath(directory: String, fileName: String): String {
    // Keep the same generated filename as the local export. The WebDAV payload
    // is encrypted internally, so the extension is only used for identification.
    return listOf(WebDavSettingsStore.normalizeDirectory(directory), fileName)
        .filter(String::isNotEmpty)
        .joinToString("/")
}

private fun formatByteSize(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> "${bytes / 1024} KB"
    else -> "%.1f MB".format(bytes / (1024.0 * 1024.0))
}

/** Categories that support choosing an import strategy (not just REPLACE). */
private val STRATEGY_CATEGORIES = setOf(
    DataCategory.MEMORY,
    DataCategory.SESSIONS,
    DataCategory.SKILLS,
    DataCategory.USER_PROFILE,
)

private val DataCategory.defaultStrategy: ImportStrategy
    get() = when (this) {
        DataCategory.CONFIG -> ImportStrategy.REPLACE
        DataCategory.MEMORY -> ImportStrategy.MERGE
        DataCategory.SESSIONS -> ImportStrategy.APPEND
        DataCategory.SKILLS -> ImportStrategy.MERGE
        DataCategory.USER_PROFILE -> ImportStrategy.MERGE
        DataCategory.PERSONALITY -> ImportStrategy.REPLACE
    }

private val DataCategory.defaultStrategies: List<ImportStrategy>
    get() = when (this) {
        DataCategory.MEMORY -> listOf(ImportStrategy.MERGE, ImportStrategy.REPLACE)
        DataCategory.SESSIONS -> listOf(ImportStrategy.APPEND, ImportStrategy.REPLACE)
        DataCategory.SKILLS -> listOf(ImportStrategy.MERGE, ImportStrategy.REPLACE)
        DataCategory.USER_PROFILE -> listOf(
            ImportStrategy.MERGE,
            ImportStrategy.APPEND,
            ImportStrategy.REPLACE,
        )
        else -> listOf(ImportStrategy.REPLACE)
    }

/** Build a summary string for the import result using localized resources. */
@Composable
private fun ImportResult.summaryString(): String {
    val parts = mutableListOf<String>()
    if (importedConfig) {
        parts.add(stringResource(R.string.data_import_summary_config))
    }
    if (importedTasks > 0) {
        parts.add(stringResource(R.string.data_import_summary_tasks, importedTasks))
    }
    if (importedMemories > 0) {
        parts.add(stringResource(R.string.data_import_summary_memories, importedMemories))
    }
    if (importedSessions > 0) {
        parts.add(stringResource(R.string.data_import_summary_sessions, importedSessions))
    }
    if (importedMessages > 0) {
        parts.add(stringResource(R.string.data_import_summary_messages, importedMessages))
    }
    if (importedSkills > 0) {
        parts.add(stringResource(R.string.data_import_summary_skills, importedSkills))
    }
    if (importedPersonalityFiles > 0) {
        parts.add(stringResource(R.string.data_import_summary_personality, importedPersonalityFiles))
    }
    if (importedProfile) {
        parts.add(stringResource(R.string.data_import_summary_user_profile, importedProfileItems))
    }
    if (skippedProfileItems > 0) {
        parts.add(stringResource(R.string.data_import_summary_user_profile_skipped, skippedProfileItems))
    }
    return parts.joinToString(", ")
}
