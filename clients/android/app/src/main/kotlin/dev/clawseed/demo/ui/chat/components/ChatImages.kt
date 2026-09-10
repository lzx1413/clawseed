package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import coil3.compose.AsyncImage
import dev.clawseed.demo.ui.chat.ChatImageDraft
import dev.clawseed.sdk.core.model.ImageAttachment
import java.io.File

@Composable
internal fun DraftImageStrip(images: List<ChatImageDraft>, file: (String) -> File, onRemove: (String) -> Unit, onRetry: (String) -> Unit) {
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        images.forEach { image ->
            key(image.id) {
                Column(Modifier.width(110.dp)) {
                    ZoomableChatImage(file(image.id))
                    if (image.preparing) { LinearProgressIndicator(Modifier.fillMaxWidth()); Text("处理中") }
                    else if (image.uploading) { LinearProgressIndicator(Modifier.fillMaxWidth()); Text("上传中") }
                    else if (image.error != null || image.attachment == null) {
                        image.error?.let { Text(it, maxLines = 2, style = MaterialTheme.typography.labelSmall) }
                        TextButton(onClick = { onRetry(image.id) }) { Text("重试") }
                    }
                    TextButton(onClick = { onRemove(image.id) }) { Text("移除图片") }
                }
            }
        }
    }
}

@Composable
internal fun MessageImageStrip(images: List<ImageAttachment>, read: (suspend (String) -> Result<ByteArray>)?) {
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        images.forEach { image ->
            key(image.id) {
                var result by remember { mutableStateOf<Result<ByteArray>?>(null) }
                var retry by remember { mutableIntStateOf(0) }
                LaunchedEffect(image.id, retry) { result = read?.invoke(image.id) ?: Result.failure(IllegalStateException("未连接")) }
                Column(Modifier.width(110.dp)) {
                    val bytes = result?.getOrNull()
                    if (bytes != null) ZoomableChatImage(bytes)
                    else if (result == null) CircularProgressIndicator(Modifier.size(28.dp))
                    else TextButton(onClick = { result = null; retry++ }) { Text("图片不可用，重试") }
                }
            }
        }
    }
}

@Composable
private fun ZoomableChatImage(data: Any) {
    var expanded by remember { mutableStateOf(false) }
    AsyncImage(model = data, contentDescription = "图片附件，点击放大", modifier = Modifier.size(110.dp).clickable { expanded = true })
    if (expanded) Dialog(onDismissRequest = { expanded = false }) {
        var scale by remember { mutableFloatStateOf(1f) }
        val gesture = rememberTransformableState { zoom, _, _ -> scale = (scale * zoom).coerceIn(1f, 5f) }
        Surface {
            Column {
                AsyncImage(model = data, contentDescription = "图片附件", modifier = Modifier.fillMaxWidth().height(480.dp)
                    .graphicsLayer(scaleX = scale, scaleY = scale).transformable(gesture))
                TextButton(onClick = { expanded = false }) { Text("关闭") }
            }
        }
    }
}
