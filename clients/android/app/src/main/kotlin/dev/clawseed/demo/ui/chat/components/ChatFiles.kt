package dev.clawseed.demo.ui.chat.components

import dev.clawseed.demo.ui.components.ClawSeedTextButton
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.res.stringResource
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.chat.ChatFileDraft
import dev.clawseed.sdk.core.model.FileAttachment

@Composable
internal fun DraftFileList(files: List<ChatFileDraft>, remove: (String) -> Unit, retry: (String) -> Unit) {
    files.forEach { file ->
        FileCard(file.name, file.format, file.size) {
            Text(when (file.status) {
                "importing" -> stringResource(R.string.chat_file_importing)
                "parsing" -> stringResource(R.string.chat_file_parsing)
                "ready" -> stringResource(R.string.chat_file_ready)
                else -> file.error ?: stringResource(R.string.chat_file_parse_failed)
            }, style = MaterialTheme.typography.bodySmall)
            Row {
                if (file.status == "failed") ClawSeedTextButton(onClick = { retry(file.id) }) { Text(stringResource(R.string.common_retry)) }
                ClawSeedTextButton(onClick = { remove(file.id) }, enabled = file.status !in setOf("importing", "parsing")) { Text(stringResource(R.string.chat_remove_file)) }
            }
        }
    }
}

@Composable
internal fun MessageFileList(files: List<FileAttachment>) {
    files.forEach { file ->
        FileCard(file.name, file.format, file.sizeBytes) {
            Text(when (file.unit) {
                "page" -> stringResource(R.string.chat_file_pages, file.total)
                "record" -> stringResource(R.string.chat_file_records, file.total)
                else -> stringResource(R.string.chat_file_characters, file.total)
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
