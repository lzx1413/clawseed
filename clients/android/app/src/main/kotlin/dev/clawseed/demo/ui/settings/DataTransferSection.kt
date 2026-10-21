package dev.clawseed.demo.ui.settings

import android.content.Intent
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
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
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
import dev.clawseed.demo.datatransfer.WebDavCheckStage
import dev.clawseed.demo.datatransfer.WebDavConnectionTestResult
import dev.clawseed.demo.datatransfer.WebDavSettings
import dev.clawseed.demo.datatransfer.WebDavSettingsStore
import dev.clawseed.demo.datatransfer.WebDavAutoBackupFrequency
import dev.clawseed.demo.datatransfer.WebDavAutoBackupSettings
import dev.clawseed.demo.datatransfer.WebDavAutoBackupScheduler
import dev.clawseed.demo.datatransfer.WebDavAutoBackupStore
import dev.clawseed.demo.datatransfer.AutoBackupDestination
import dev.clawseed.demo.scheduled.ScheduledTaskStore
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
    val webDavAutoBackupStore = remember { WebDavAutoBackupStore(context) }

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
    var webDavAutoBackupEnabled by remember { mutableStateOf(false) }
    var webDavAutoBackupFrequency by remember { mutableStateOf(WebDavAutoBackupFrequency.DAILY) }
    var webDavAutoBackupDestination by remember { mutableStateOf(AutoBackupDestination.WEBDAV) }
    var webDavAutoBackupLocalDirectoryUri by remember { mutableStateOf<String?>(null) }
    var webDavAutoBackupWifiOnly by remember { mutableStateOf(true) }
    var webDavAutoBackupRetainCount by remember { mutableStateOf("5") }
    var webDavAutoBackupHour by remember { mutableStateOf("03") }
    var webDavAutoBackupMinute by remember { mutableStateOf("00") }
    var webDavAutoBackupWeeklyDay by remember { mutableStateOf(1) }
    var webDavAutoBackupLastSuccessAt by remember { mutableStateOf<Long?>(null) }
    var webDavAutoBackupLastFailure by remember { mutableStateOf<String?>(null) }
    var webDavAutoBackupRetryCount by remember { mutableStateOf(0) }
    var webDavAutoBackupRunning by remember { mutableStateOf(false) }
    var webDavSettingsLoaded by remember { mutableStateOf(false) }
    var webDavBusy by remember { mutableStateOf(false) }
    var webDavStatus by remember { mutableStateOf<String?>(null) }
    var autoBackupStatus by remember { mutableStateOf<String?>(null) }
    var webDavBackups by remember { mutableStateOf(emptyList<WebDavSyncClient.RemoteBackup>()) }
    var webDavBackupsLoading by remember { mutableStateOf(false) }
    var webDavListError by remember { mutableStateOf<String?>(null) }
    var webDavDeleteCandidate by remember { mutableStateOf<WebDavSyncClient.RemoteBackup?>(null) }
    var webDavTesting by remember { mutableStateOf(false) }
    var webDavTestResult by remember { mutableStateOf<WebDavConnectionTestResult?>(null) }
    var webDavTestError by remember { mutableStateOf<String?>(null) }
    var mediaCacheExpanded by remember { mutableStateOf(false) }
    var exportExpanded by remember { mutableStateOf(false) }
    var autoBackupExpanded by remember { mutableStateOf(false) }
    var webDavExpanded by remember { mutableStateOf(false) }
    var importExpanded by remember { mutableStateOf(false) }
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
        webDavEncryptionPassword = settings.encryptionPassword
        rememberWebDavPassword = settings.rememberPassword
        val auto = withContext(Dispatchers.IO) { webDavAutoBackupStore.load() }
        webDavAutoBackupEnabled = auto.enabled
        webDavAutoBackupFrequency = auto.frequency
        webDavAutoBackupDestination = auto.destination
        webDavAutoBackupLocalDirectoryUri = auto.localDirectoryUri
        webDavAutoBackupWifiOnly = auto.wifiOnly
        webDavAutoBackupRetainCount = auto.retainCount.toString()
        webDavAutoBackupHour = auto.hour.toString().padStart(2, '0')
        webDavAutoBackupMinute = auto.minute.toString().padStart(2, '0')
        webDavAutoBackupWeeklyDay = auto.weeklyDay
        webDavAutoBackupLastSuccessAt = auto.lastSuccessAt
        webDavAutoBackupLastFailure = auto.lastFailure
        webDavAutoBackupRetryCount = auto.retryCount
        webDavSettingsLoaded = true
    }

    LaunchedEffect(webDavAutoBackupRunning) {
        if (!webDavAutoBackupRunning) return@LaunchedEffect
        val before = withContext(Dispatchers.IO) { webDavAutoBackupStore.load() }
        var completed = false
        for (attempt in 0 until 300) {
            kotlinx.coroutines.delay(1_000)
            val current = withContext(Dispatchers.IO) { webDavAutoBackupStore.load() }
            if (current.lastSuccessAt != before.lastSuccessAt || current.retryCount > before.retryCount) {
                webDavAutoBackupLastSuccessAt = current.lastSuccessAt
                webDavAutoBackupLastFailure = current.lastFailure
                webDavAutoBackupRetryCount = current.retryCount
                webDavAutoBackupRunning = false
                webDavStatus = if (current.lastSuccessAt != before.lastSuccessAt) {
                    context.getString(R.string.data_webdav_auto_run_done)
                } else {
                    context.getString(R.string.data_webdav_auto_run_failed, current.lastFailure.orEmpty())
                }
                completed = true
                break
            }
        }
        if (!completed && webDavAutoBackupRunning) {
            webDavAutoBackupRunning = false
            webDavStatus = context.getString(R.string.data_webdav_auto_run_timeout)
        }
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
                    encryptionPassword = webDavEncryptionPassword,
                    rememberEncryptionPassword = webDavAutoBackupEnabled && webDavAutoBackupDestination != AutoBackupDestination.LOCAL,
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

    // Import state
    var selectedImportCategories by remember { mutableStateOf(emptySet<DataCategory>()) }
    var importStrategies by remember { mutableStateOf(mapOf<DataCategory, ImportStrategy>()) }
    var localBackups by remember { mutableStateOf(emptyList<DataTransferManager.LocalBackup>()) }
    var localBackupsLoading by remember { mutableStateOf(false) }
    var localSelectedFile by remember { mutableStateOf<DataTransferManager.LocalBackup?>(null) }
    var isImporting by remember { mutableStateOf(false) }
    var importResult by remember { mutableStateOf<ImportResult?>(null) }
    var importError by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(importSource, webDavSettingsLoaded, webDavAutoBackupLocalDirectoryUri) {
        if (importSource == TransferEndpoint.WebDav && webDavSettingsLoaded) {
            refreshWebDavBackups()
        } else if (importSource == TransferEndpoint.LocalFile && webDavSettingsLoaded) {
            localBackupsLoading = true
            val result = webDavAutoBackupLocalDirectoryUri?.let { uri ->
                withContext(Dispatchers.IO) { manager.listLocalBackups(Uri.parse(uri)) }
            }
            localBackups = result?.getOrElse { emptyList() } ?: emptyList()
            localSelectedFile = localBackups.firstOrNull()
            localBackupsLoading = false
        }
    }

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
    val autoBackupSettingsSaved = stringResource(R.string.data_webdav_auto_settings_saved)
    val webDavDeleteDone = stringResource(R.string.data_webdav_delete_done)
    val webDavTestPassed = stringResource(R.string.data_webdav_test_passed)
    val webDavTestFailed = stringResource(R.string.data_webdav_test_failed)

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

    val localDirectoryLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        try {
            context.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
            webDavAutoBackupLocalDirectoryUri = uri.toString()
            scope.launch {
                try {
                    withContext(Dispatchers.IO) {
                        webDavAutoBackupStore.save(
                            webDavAutoBackupStore.load().copy(localDirectoryUri = uri.toString()),
                        )
                    }
                    autoBackupStatus = context.getString(R.string.data_webdav_auto_local_directory_selected)
                } catch (error: Exception) {
                    autoBackupStatus = error.message ?: context.getString(R.string.data_webdav_auto_local_directory_missing)
                }
            }
        } catch (error: Exception) {
            autoBackupStatus = error.message ?: context.getString(R.string.data_webdav_auto_local_directory_missing)
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        DataTransferSubsectionHeader(
            title = stringResource(R.string.data_media_cache),
            expanded = mediaCacheExpanded,
            onToggle = { mediaCacheExpanded = !mediaCacheExpanded },
        )
        if (mediaCacheExpanded) {
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

        }

        DataTransferSubsectionHeader(
            title = stringResource(R.string.data_export),
            expanded = exportExpanded,
            onToggle = { exportExpanded = !exportExpanded },
        )
        if (exportExpanded) {
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

        }

        DataTransferSubsectionHeader(
            title = stringResource(R.string.data_webdav_auto_title),
            expanded = autoBackupExpanded,
            onToggle = { autoBackupExpanded = !autoBackupExpanded },
        )
        if (autoBackupExpanded) {
        // Scheduled backup policy card
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.data_webdav_auto_title), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.data_webdav_auto_desc), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                    Switch(checked = webDavAutoBackupEnabled, onCheckedChange = { webDavAutoBackupEnabled = it })
                    Spacer(modifier = Modifier.width(4.dp))
                    Text(stringResource(R.string.data_webdav_auto_enable))
                }
                if (webDavAutoBackupEnabled) {
                    Text(stringResource(R.string.data_webdav_auto_destination), style = MaterialTheme.typography.labelLarge)
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.horizontalScroll(rememberScrollState()),
                    ) {
                        FilterChip(
                            selected = webDavAutoBackupDestination == AutoBackupDestination.LOCAL,
                            onClick = { webDavAutoBackupDestination = AutoBackupDestination.LOCAL },
                            label = { Text(stringResource(R.string.data_webdav_auto_destination_local)) },
                        )
                        FilterChip(
                            selected = webDavAutoBackupDestination == AutoBackupDestination.WEBDAV,
                            onClick = { webDavAutoBackupDestination = AutoBackupDestination.WEBDAV },
                            label = { Text(stringResource(R.string.data_webdav_auto_destination_webdav)) },
                        )
                        FilterChip(
                            selected = webDavAutoBackupDestination == AutoBackupDestination.LOCAL_AND_WEBDAV,
                            onClick = { webDavAutoBackupDestination = AutoBackupDestination.LOCAL_AND_WEBDAV },
                            label = { Text(stringResource(R.string.data_webdav_auto_destination_both)) },
                        )
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        TextField(
                            value = webDavAutoBackupHour,
                            onValueChange = { webDavAutoBackupHour = it.filter(Char::isDigit).take(2) },
                            label = { Text(stringResource(R.string.data_webdav_auto_hour)) },
                            singleLine = true,
                            modifier = Modifier.weight(1f),
                        )
                        Text(":", style = MaterialTheme.typography.titleLarge)
                        TextField(
                            value = webDavAutoBackupMinute,
                            onValueChange = { webDavAutoBackupMinute = it.filter(Char::isDigit).take(2) },
                            label = { Text(stringResource(R.string.data_webdav_auto_minute)) },
                            singleLine = true,
                            modifier = Modifier.weight(1f),
                        )
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(
                            selected = webDavAutoBackupFrequency == WebDavAutoBackupFrequency.DAILY,
                            onClick = { webDavAutoBackupFrequency = WebDavAutoBackupFrequency.DAILY },
                            label = { Text(stringResource(R.string.data_webdav_auto_daily)) },
                        )
                        FilterChip(
                            selected = webDavAutoBackupFrequency == WebDavAutoBackupFrequency.WEEKLY,
                            onClick = { webDavAutoBackupFrequency = WebDavAutoBackupFrequency.WEEKLY },
                            label = { Text(stringResource(R.string.data_webdav_auto_weekly)) },
                        )
                    }
                    if (webDavAutoBackupFrequency == WebDavAutoBackupFrequency.WEEKLY) {
                        Text(stringResource(R.string.data_webdav_auto_weekly_day), style = MaterialTheme.typography.labelLarge)
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                            modifier = Modifier.horizontalScroll(rememberScrollState()),
                        ) {
                            (1..7).forEach { day ->
                                FilterChip(
                                    selected = webDavAutoBackupWeeklyDay == day,
                                    onClick = { webDavAutoBackupWeeklyDay = day },
                                    label = { Text(stringResource(weekdayLabelRes(day))) },
                                )
                            }
                        }
                    }
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                        Switch(checked = webDavAutoBackupWifiOnly, onCheckedChange = { webDavAutoBackupWifiOnly = it })
                        Spacer(modifier = Modifier.width(4.dp))
                        Text(stringResource(R.string.data_webdav_auto_wifi_only))
                    }
                    TextField(
                        value = webDavAutoBackupRetainCount,
                        onValueChange = { webDavAutoBackupRetainCount = it.filter(Char::isDigit).take(3) },
                        label = { Text(stringResource(R.string.data_webdav_auto_retain)) },
                        supportingText = { Text(stringResource(R.string.data_webdav_auto_retain_hint)) },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    if (webDavAutoBackupDestination != AutoBackupDestination.LOCAL) {
                        Text(stringResource(R.string.data_webdav_auto_password_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (webDavAutoBackupDestination != AutoBackupDestination.WEBDAV) {
                        Text(
                            stringResource(
                                if (webDavAutoBackupLocalDirectoryUri == null) {
                                    R.string.data_webdav_auto_local_directory_missing
                                } else {
                                    R.string.data_webdav_auto_local_path
                                },
                                webDavAutoBackupLocalDirectoryUri.orEmpty(),
                            ),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Button(
                            onClick = { localDirectoryLauncher.launch(null) },
                            enabled = !webDavBusy,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(stringResource(R.string.data_webdav_auto_select_local_directory))
                        }
                    }
                    webDavAutoBackupLastSuccessAt?.let { timestamp ->
                        Text(
                            stringResource(R.string.data_webdav_auto_last_success, formatAutoBackupTime(timestamp)),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    webDavAutoBackupLastFailure?.let { failure ->
                        Text(
                            stringResource(R.string.data_webdav_auto_last_failure, failure, webDavAutoBackupRetryCount),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                    Button(
                        onClick = {
                            scope.launch {
                                val taskExists = withContext(Dispatchers.IO) {
                                    ScheduledTaskStore(context).tasksAsList().any {
                                        it.id == WebDavAutoBackupScheduler.TASK_ID && it.enabled && it.isWebDavBackup
                                    }
                                }
                                if (!taskExists) {
                                    autoBackupStatus = context.getString(R.string.data_webdav_auto_save_first)
                                } else {
                                    autoBackupStatus = context.getString(R.string.data_webdav_auto_running)
                                    webDavAutoBackupRunning = true
                                    WebDavAutoBackupScheduler.triggerNow(context)
                                }
                            }
                        },
                        enabled = !webDavBusy && !webDavAutoBackupRunning,
                        modifier = Modifier.fillMaxWidth(),
                    ) { Text(stringResource(if (webDavAutoBackupRunning) R.string.data_webdav_auto_running else R.string.data_webdav_auto_run_now)) }
                }
                Button(
                    onClick = {
                        scope.launch {
                            val retainCount = webDavAutoBackupRetainCount.toIntOrNull()
                            val hour = webDavAutoBackupHour.toIntOrNull()
                            val minute = webDavAutoBackupMinute.toIntOrNull()
                            if (webDavAutoBackupEnabled && webDavAutoBackupDestination != AutoBackupDestination.LOCAL && webDavEncryptionPassword.length < 8) {
                                autoBackupStatus = context.getString(R.string.data_webdav_auto_password_required)
                                return@launch
                            }
                            if (webDavAutoBackupEnabled && webDavAutoBackupDestination != AutoBackupDestination.WEBDAV && webDavAutoBackupLocalDirectoryUri.isNullOrBlank()) {
                                autoBackupStatus = context.getString(R.string.data_webdav_auto_local_directory_required)
                                return@launch
                            }
                            if (webDavAutoBackupEnabled && (retainCount == null || retainCount !in 1..100)) {
                                autoBackupStatus = context.getString(R.string.data_webdav_auto_retain_invalid)
                                return@launch
                            }
                            if (webDavAutoBackupEnabled && (hour == null || hour !in 0..23 || minute == null || minute !in 0..59)) {
                                autoBackupStatus = context.getString(R.string.data_webdav_auto_time_invalid)
                                return@launch
                            }
                            saveWebDavSettings()
                            val auto = WebDavAutoBackupSettings(
                                enabled = webDavAutoBackupEnabled,
                                frequency = webDavAutoBackupFrequency,
                                destination = webDavAutoBackupDestination,
                                localDirectoryUri = webDavAutoBackupLocalDirectoryUri,
                                wifiOnly = webDavAutoBackupWifiOnly,
                                retainCount = retainCount ?: 5,
                                hour = hour ?: 3,
                                minute = minute ?: 0,
                                weeklyDay = webDavAutoBackupWeeklyDay,
                            )
                            withContext(Dispatchers.IO) { webDavAutoBackupStore.save(auto) }
                            WebDavAutoBackupScheduler.apply(context, auto)
                            autoBackupStatus = autoBackupSettingsSaved
                        }
                    },
                    enabled = webDavSettingsLoaded && !webDavBusy,
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(R.string.data_webdav_auto_save_settings)) }
                autoBackupStatus?.let { status ->
                    Text(
                        status,
                        color = if (status == autoBackupSettingsSaved) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }

        }

        DataTransferSubsectionHeader(
            title = stringResource(R.string.data_webdav_title),
            expanded = webDavExpanded,
            onToggle = { webDavExpanded = !webDavExpanded },
        )
        if (webDavExpanded) {
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
                ) { Text(stringResource(R.string.data_webdav_save_connection)) }
                Button(
                    onClick = {
                        scope.launch {
                            webDavTesting = true
                            webDavTestResult = null
                            webDavTestError = null
                            val result = manager.testWebDavConnection(
                                webDavUrl,
                                WebDavSettingsStore.normalizeDirectory(webDavDirectory),
                                webDavUsername.takeIf(String::isNotBlank),
                                webDavPassword.takeIf(String::isNotBlank),
                            )
                            if (result.isSuccess) {
                                webDavTestResult = result.getOrThrow()
                            } else {
                                webDavTestError = result.exceptionOrNull()?.message ?: ""
                            }
                            webDavTesting = false
                        }
                    },
                    enabled = webDavSettingsLoaded && !webDavBusy && !webDavTesting,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (webDavTesting) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                        Spacer(Modifier.width(8.dp))
                    }
                    Text(
                        if (webDavTesting) stringResource(R.string.data_webdav_testing_connection)
                        else stringResource(R.string.data_webdav_test_connection),
                    )
                }
                webDavTestResult?.let { testResult ->
                    Text(
                        if (testResult.passed) webDavTestPassed else webDavTestFailed,
                        color = if (testResult.passed) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    for (check in testResult.checks) {
                        val stage = when (check.stage) {
                            WebDavCheckStage.URL -> stringResource(R.string.data_webdav_check_url)
                            WebDavCheckStage.AUTHENTICATION -> stringResource(R.string.data_webdav_check_authentication)
                            WebDavCheckStage.DIRECTORY -> stringResource(R.string.data_webdav_check_directory)
                            WebDavCheckStage.WRITE -> stringResource(R.string.data_webdav_check_write)
                        }
                        Text(
                            stringResource(R.string.data_webdav_check_line, stage, check.detail),
                            color = if (check.passed) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
                webDavTestError?.let { error ->
                    Text(
                        stringResource(R.string.data_webdav_test_failed_with_error, error),
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
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

        }

        DataTransferSubsectionHeader(
            title = stringResource(R.string.data_import),
            expanded = importExpanded,
            onToggle = { importExpanded = !importExpanded },
        )
        if (importExpanded) {
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

                if (importSource == TransferEndpoint.LocalFile) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(stringResource(R.string.data_local_backups_title), style = MaterialTheme.typography.labelLarge)
                        Button(
                            onClick = {
                                scope.launch {
                                    localBackupsLoading = true
                                    val result = webDavAutoBackupLocalDirectoryUri?.let { uri ->
                                        withContext(Dispatchers.IO) { manager.listLocalBackups(Uri.parse(uri)) }
                                    }
                                    localBackups = result?.getOrElse { emptyList() } ?: emptyList()
                                    localSelectedFile = localSelectedFile?.takeIf { it in localBackups } ?: localBackups.firstOrNull()
                                    localBackupsLoading = false
                                }
                            },
                            enabled = !localBackupsLoading && !webDavBusy,
                        ) {
                            if (localBackupsLoading) {
                                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                                Spacer(Modifier.width(6.dp))
                            }
                            Text(stringResource(R.string.data_webdav_refresh_backups))
                        }
                    }
                    if (!localBackupsLoading && localBackups.isEmpty()) {
                        Text(
                            stringResource(R.string.data_local_backups_empty),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    for (backup in localBackups) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { localSelectedFile = backup },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(
                                selected = localSelectedFile == backup,
                                onClick = { localSelectedFile = backup },
                            )
                            Column(modifier = Modifier.weight(1f).padding(start = 4.dp)) {
                                Text(backup.name, style = MaterialTheme.typography.bodyMedium)
                                Text(
                                    stringResource(R.string.data_webdav_backup_details, formatByteSize(backup.bytes), formatAutoBackupTime(backup.lastModified)),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
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
                        if (importSource == TransferEndpoint.LocalFile && localSelectedFile == null) {
                            importLauncher.launch(arrayOf("application/zip", "*/*"))
                        } else if (importSource == TransferEndpoint.LocalFile) {
                            scope.launch {
                                isImporting = true
                                importError = null
                                importResult = null
                                try {
                                    val result = withContext(Dispatchers.IO) {
                                        manager.restoreFromLocalBackup(localSelectedFile!!, selectedImportCategories, importStrategies)
                                            .getOrElse { throw it }
                                    }
                                    importResult = result
                                } catch (e: Exception) {
                                    importError = importFailedFormat.format(e.message ?: "")
                                } finally {
                                    isImporting = false
                                }
                            }
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

@Composable
private fun DataTransferSubsectionHeader(
    title: String,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onToggle),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(if (expanded) "▾" else "▸", style = MaterialTheme.typography.titleMedium)
        }
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

private fun formatAutoBackupTime(timestamp: Long): String =
    SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.getDefault()).format(Date(timestamp))

private fun weekdayLabelRes(day: Int): Int = when (day) {
    1 -> R.string.data_webdav_auto_monday
    2 -> R.string.data_webdav_auto_tuesday
    3 -> R.string.data_webdav_auto_wednesday
    4 -> R.string.data_webdav_auto_thursday
    5 -> R.string.data_webdav_auto_friday
    6 -> R.string.data_webdav_auto_saturday
    else -> R.string.data_webdav_auto_sunday
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
