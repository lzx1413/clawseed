package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.chat.ChatFileDraft
import dev.clawseed.sdk.core.model.FileAttachment

@Composable
internal fun DraftFileList(files: List<ChatFileDraft>, remove: (String) -> Unit, retry: (String) -> Unit) {
    files.forEach { file ->
        FileCard(file.name, file.format, file.size) {
            Text(when (file.status) {
                "importing" -> "正在导入…"
                "parsing" -> "正在解析…"
                "ready" -> "可发送"
                else -> file.error ?: "解析失败"
            }, style = MaterialTheme.typography.bodySmall)
            Row {
                if (file.status == "failed") TextButton(onClick = { retry(file.id) }) { Text("重试") }
                TextButton(onClick = { remove(file.id) }, enabled = file.status !in setOf("importing", "parsing")) { Text("移除") }
            }
        }
    }
}

@Composable
internal fun MessageFileList(files: List<FileAttachment>) {
    files.forEach { file ->
        FileCard(file.name, file.format, file.sizeBytes) {
            Text(when (file.unit) {
                "page" -> "${file.total} 页"
                "record" -> "${file.total} 条记录（含表头）"
                else -> "${file.total} 字符"
            }, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun FileCard(name: String, format: String, bytes: Long, content: @Composable () -> Unit) {
    Surface(modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 3.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh, shape = MaterialTheme.shapes.medium) {
        Column(Modifier.padding(10.dp)) {
            Text(name, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.titleSmall)
            Text("${format.uppercase()} · ${android.text.format.Formatter.formatShortFileSize(androidx.compose.ui.platform.LocalContext.current, bytes)}", style = MaterialTheme.typography.bodySmall)
            content()
        }
    }
}
