package dev.clawseed.demo.ui.settings

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.datatransfer.DataCategory
import dev.clawseed.demo.datatransfer.DataTransferManager
import dev.clawseed.demo.datatransfer.ImportResult
import dev.clawseed.demo.datatransfer.ImportStrategy
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

    // Export state
    var selectedExportCategories by remember { mutableStateOf(emptySet<DataCategory>()) }
    var excludeSensitive by remember { mutableStateOf(true) }
    var isExporting by remember { mutableStateOf(false) }
    var exportError by remember { mutableStateOf<String?>(null) }
    var exportSuccess by remember { mutableStateOf<String?>(null) }

    // Import state
    var selectedImportCategories by remember { mutableStateOf(emptySet<DataCategory>()) }
    var importStrategies by remember { mutableStateOf(mapOf<DataCategory, ImportStrategy>()) }
    var isImporting by remember { mutableStateOf(false) }
    var importResult by remember { mutableStateOf<ImportResult?>(null) }
    var importError by remember { mutableStateOf<String?>(null) }

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
                exportSuccess = "导出成功！已导出 ${selectedExportCategories.map { it.label }.joinToString(", ")}"
            } catch (e: Exception) {
                exportError = "导出失败: ${e.message}"
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
                    } ?: throw IllegalStateException("无法读取文件")
                }
                importResult = result
            } catch (e: Exception) {
                importError = "导入失败: ${e.message}"
            } finally {
                isImporting = false
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // Export card
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("导出数据", style = MaterialTheme.typography.titleMedium)

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
                            Text(category.label, style = MaterialTheme.typography.bodyMedium)
                            Text(
                                category.description,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
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
                        Text("排除敏感数据", style = MaterialTheme.typography.bodyMedium)
                        Text(
                            "不导出 API 密钥和认证令牌",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                        )
                    }
                }

                // Export button
                Button(
                    onClick = {
                        val date = SimpleDateFormat("yyyyMMdd", Locale.getDefault()).format(Date())
                        val cats = selectedExportCategories.joinToString("-") { it.name.lowercase() }
                        exportLauncher.launch("clawseed-${date}-${cats}.zip")
                    },
                    enabled = selectedExportCategories.isNotEmpty() && !isExporting,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (isExporting) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("导出中...")
                    } else {
                        Text("导出")
                    }
                }

                // Status messages
                exportSuccess?.let {
                    Text(it, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodyMedium)
                }
                exportError?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }

        // Import card
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("导入数据", style = MaterialTheme.typography.titleMedium)

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
                            Text(category.label, style = MaterialTheme.typography.bodyMedium)
                            Text(
                                category.description,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
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
                                    label = { Text(strategy.label) },
                                )
                            }
                        }
                    }
                }

                // Warning text
                Text(
                    "导入将修改应用数据。建议先导出备份。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                )

                // Import button
                Button(
                    onClick = {
                        importLauncher.launch(arrayOf("application/zip", "*/*"))
                    },
                    enabled = selectedImportCategories.isNotEmpty() && !isImporting,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    if (isImporting) {
                        CircularProgressIndicator(modifier = Modifier.size(18.dp), color = MaterialTheme.colorScheme.onPrimary)
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("导入中...")
                    } else {
                        Text("选择文件并导入")
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
            title = { Text(if (result.isSuccess) "导入完成" else "导入出现问题") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(result.summary, style = MaterialTheme.typography.bodyMedium)
                    if (result.warnings.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text("警告:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        for (w in result.warnings) {
                            Text(w, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                    if (result.errors.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(8.dp))
                        Text("错误:", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                        for (e in result.errors) {
                            Text(e, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                        }
                    }
                }
            },
            confirmButton = {
                Button(onClick = { importResult = null }) { Text("确定") }
            },
        )
    }
}

/** Categories that support choosing an import strategy (not just REPLACE). */
private val STRATEGY_CATEGORIES = setOf(DataCategory.MEMORY, DataCategory.SESSIONS, DataCategory.SKILLS)

private val DataCategory.defaultStrategy: ImportStrategy
    get() = when (this) {
        DataCategory.CONFIG -> ImportStrategy.REPLACE
        DataCategory.MEMORY -> ImportStrategy.MERGE
        DataCategory.SESSIONS -> ImportStrategy.APPEND
        DataCategory.SKILLS -> ImportStrategy.MERGE
        DataCategory.PERSONALITY -> ImportStrategy.REPLACE
    }

private val DataCategory.defaultStrategies: List<ImportStrategy>
    get() = when (this) {
        DataCategory.MEMORY -> listOf(ImportStrategy.MERGE, ImportStrategy.REPLACE)
        DataCategory.SESSIONS -> listOf(ImportStrategy.APPEND, ImportStrategy.REPLACE)
        DataCategory.SKILLS -> listOf(ImportStrategy.MERGE, ImportStrategy.REPLACE)
        else -> listOf(ImportStrategy.REPLACE)
    }
