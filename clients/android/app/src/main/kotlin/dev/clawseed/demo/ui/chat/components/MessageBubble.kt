package dev.clawseed.demo.ui.chat.components

import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.compose.foundation.Image
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.theme.success
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.ToolCallInfo
import dev.clawseed.demo.ui.chat.rememberRichMediaImageLoader
import dev.clawseed.sdk.core.model.ContentBlock
import dev.clawseed.sdk.core.model.MediaReference
import dev.clawseed.sdk.core.model.ToolPresentation
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import coil3.compose.AsyncImage
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

@Composable
fun MessageBubble(
    entry: ChatEntry,
    modifier: Modifier = Modifier,
    onRegenerate: (() -> Unit)? = null,
    onSpeak: ((String) -> Unit)? = null,
    onStop: (() -> Unit)? = null,
    onPresentationAction: ((String) -> Unit)? = null,
    isSpeakingThis: Boolean = false,
    onReadImage: (suspend (String) -> Result<ByteArray>)? = null,
) {
    when (entry) {
        is ChatEntry.UserMessage -> Column(
            modifier = modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.End,
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (entry.attachments.isNotEmpty()) {
                Box(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                    contentAlignment = Alignment.TopEnd,
                ) {
                    MessageImageStrip(entry.attachments, onReadImage)
                }
            }
            MessageFileList(entry.files)
            if (entry.content.isNotBlank()) UserBubble(entry.content)
        }
        is ChatEntry.AssistantMessage -> AssistantBubble(
            content = entry.content,
            metrics = entry.metrics,
            isStreaming = entry.isStreaming,
            presentation = entry.presentation,
            onRegenerate = onRegenerate,
            onSpeak = onSpeak,
            onStop = onStop,
            onPresentationAction = onPresentationAction,
            isSpeakingThis = isSpeakingThis,
            modifier = modifier,
        )
        is ChatEntry.ToolInvocations -> ToolInvocationsCard(entry, modifier)
        is ChatEntry.Thinking -> ThinkingCard(entry.content, modifier)
        is ChatEntry.SystemMessage -> SystemBubble(entry.content, modifier)
        is ChatEntry.DebugInfo -> DebugInfoCard(entry, modifier)
    }
}

@Composable
private fun UserBubble(content: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.End,
    ) {
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp))
                .background(MaterialTheme.colorScheme.surfaceContainerHigh)
                .padding(horizontal = 16.dp, vertical = 10.dp),
        ) {
            val selectionColors = TextSelectionColors(
                handleColor = MaterialTheme.colorScheme.primary,
                backgroundColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.3f),
            )
            CompositionLocalProvider(LocalTextSelectionColors provides selectionColors) {
                SelectionContainer {
                    Text(
                        text = content,
                        color = MaterialTheme.colorScheme.onSurface,
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
            }
        }
    }
}

@Composable
private fun AssistantBubble(
    content: String,
    metrics: dev.clawseed.sdk.core.model.ResponseMetrics?,
    isStreaming: Boolean,
    presentation: dev.clawseed.sdk.core.model.ToolPresentation?,
    onRegenerate: (() -> Unit)?,
    onSpeak: ((String) -> Unit)?,
    onStop: (() -> Unit)?,
    onPresentationAction: ((String) -> Unit)?,
    isSpeakingThis: Boolean,
    modifier: Modifier = Modifier,
) {
    val clipboardManager = LocalClipboardManager.current
    val context = LocalContext.current
    val copiedText = stringResource(R.string.common_copied)

    Column(modifier = modifier.fillMaxWidth()) {
        SelectionContainer {
            Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                MarkdownContent(content = content, cacheDocument = !isStreaming)
                presentation?.let { RichContentBlocks(it, onPresentationAction) }
                if (isStreaming) {
                    Text(
                        text = "█",
                        color = MaterialTheme.colorScheme.primary,
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
            }
        }
        if (!isStreaming && metrics != null) {
            val unavailable = stringResource(R.string.metrics_unavailable)
            val ratio = metrics.cacheHitRatio?.takeIf { it.isFinite() && it in 0.0..1.0 }
                ?.let { String.format(java.util.Locale.getDefault(), "%.1f%%", it * 100) } ?: unavailable
            val speed = metrics.outputTokensPerSecond?.takeIf { it.isFinite() && it >= 0 }
                ?.let { String.format(java.util.Locale.getDefault(), "%.1f tok/s", it) } ?: unavailable
            val elapsed = metrics.elapsedMs?.takeIf { it >= 0 }
                ?.let { String.format(java.util.Locale.getDefault(), "%.2f s", it / 1000.0) } ?: unavailable
            Text(
                text = stringResource(R.string.msg_response_metrics,
                    metrics.inputTokens?.toString() ?: unavailable,
                    metrics.outputTokens?.toString() ?: unavailable, ratio, speed, elapsed),
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (!isStreaming && content.isNotBlank()) {
            Row(
                modifier = Modifier.padding(start = 16.dp, top = 2.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (onSpeak != null && onStop != null) {
                    SpeakButton(
                        content = content,
                        isSpeaking = isSpeakingThis,
                        onSpeak = onSpeak,
                        onStop = onStop,
                    )
                }
                CopyButton(
                    onClick = {
                        clipboardManager.setText(AnnotatedString(content))
                        Toast.makeText(context, copiedText, Toast.LENGTH_SHORT).show()
                    },
                )
                if (onRegenerate != null) {
                    RegenerateButton(onClick = onRegenerate)
                }
            }
        }
    }
}

@Composable
private fun SpeakButton(
    content: String,
    isSpeaking: Boolean,
    onSpeak: (String) -> Unit,
    onStop: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(4.dp))
            .clickable {
                if (isSpeaking) onStop() else onSpeak(content)
            }
            .padding(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = if (isSpeaking) SpeakerStopIcon else SpeakerPlayIcon,
            contentDescription = stringResource(if (isSpeaking) R.string.msg_stop_speaking else R.string.msg_speak),
            modifier = Modifier.size(14.dp),
            tint = if (isSpeaking) MaterialTheme.colorScheme.primary
                   else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun CopyButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    var copied by remember { mutableStateOf(false) }

    LaunchedEffect(copied) {
        if (copied) {
            delay(2000)
            copied = false
        }
    }

    Row(
        modifier = modifier
            .clip(RoundedCornerShape(4.dp))
            .clickable {
                if (!copied) {
                    copied = true
                    onClick()
                }
            }
            .padding(4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Icon(
            imageVector = CopyIcon,
            contentDescription = stringResource(R.string.msg_copy),
            modifier = Modifier.size(14.dp),
            tint = if (copied) MaterialTheme.colorScheme.success
                   else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (copied) {
            Text(
                text = stringResource(R.string.msg_copied),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.success,
            )
        }
    }
}

@Composable
private fun RegenerateButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(4.dp))
            .clickable { onClick() }
            .padding(4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Icon(
            imageVector = RefreshIcon,
            contentDescription = stringResource(R.string.msg_regenerate),
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private val RefreshIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "Refresh",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(17.65f, 6.35f)
            curveTo(16.2f, 4.9f, 14.21f, 4f, 12f, 4f)
            curveTo(7.58f, 4f, 4.01f, 7.58f, 4.01f, 12f)
            curveTo(4.01f, 16.42f, 7.58f, 20f, 12f, 20f)
            curveTo(15.73f, 20f, 18.84f, 17.45f, 19.73f, 14f)
            horizontalLineTo(17.65f)
            curveTo(16.83f, 16.33f, 14.61f, 18f, 12f, 18f)
            curveTo(8.69f, 18f, 6f, 15.31f, 6f, 12f)
            curveTo(6f, 8.69f, 8.69f, 6f, 12f, 6f)
            curveTo(13.66f, 6f, 15.14f, 6.69f, 16.22f, 7.78f)
            lineTo(13f, 11f)
            horizontalLineTo(20f)
            verticalLineTo(4f)
            close()
        }
    }.build()
}

private val CopyIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "ContentCopy",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(16f, 1f)
            horizontalLineTo(4f)
            curveTo(2.9f, 1f, 2f, 1.9f, 2f, 3f)
            verticalLineTo(17f)
            horizontalLineTo(4f)
            verticalLineTo(3f)
            horizontalLineTo(16f)
            close()
            moveTo(19f, 5f)
            horizontalLineTo(8f)
            curveTo(6.9f, 5f, 6f, 5.9f, 6f, 7f)
            verticalLineTo(21f)
            curveTo(6f, 22.1f, 6.9f, 23f, 8f, 23f)
            horizontalLineTo(19f)
            curveTo(20.1f, 23f, 21f, 22.1f, 21f, 21f)
            verticalLineTo(7f)
            curveTo(21f, 5.9f, 20.1f, 5f, 19f, 5f)
            close()
            moveTo(19f, 21f)
            horizontalLineTo(8f)
            verticalLineTo(7f)
            horizontalLineTo(19f)
            verticalLineTo(21f)
            close()
        }
    }.build()
}

val SpeakerPlayIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "SpeakerPlay",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            // Speaker cone + three sound waves
            moveTo(3f, 9f)
            verticalLineTo(15f)
            horizontalLineTo(7f)
            lineTo(12f, 20f)
            verticalLineTo(4f)
            lineTo(7f, 9f)
            close()
            moveTo(16f, 3f)
            curveTo(18.3f, 4.7f, 19.5f, 7.3f, 19.5f, 12f)
            curveTo(19.5f, 16.7f, 18.3f, 19.3f, 16f, 21f)
            lineTo(15f, 20f)
            curveTo(17f, 18.5f, 17.8f, 16.2f, 17.8f, 12f)
            curveTo(17.8f, 7.8f, 17f, 5.5f, 15f, 4f)
            close()
            moveTo(20f, 1f)
            curveTo(22.5f, 3.5f, 24f, 7.3f, 24f, 12f)
            curveTo(24f, 16.7f, 22.5f, 20.5f, 20f, 23f)
            lineTo(19f, 22f)
            curveTo(21.2f, 19.8f, 22.5f, 16.3f, 22.5f, 12f)
            curveTo(22.5f, 7.7f, 21.2f, 4.2f, 19f, 2f)
            close()
        }
    }.build()
}

val SpeakerStopIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "SpeakerStop",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            // Speaker cone (no waves) + a square "stop" mark
            moveTo(3f, 9f)
            verticalLineTo(15f)
            horizontalLineTo(7f)
            lineTo(12f, 20f)
            verticalLineTo(4f)
            lineTo(7f, 9f)
            close()
            moveTo(16f, 10f)
            horizontalLineTo(22f)
            verticalLineTo(14f)
            horizontalLineTo(16f)
            close()
        }
    }.build()
}

/** Speaker with a slash — speech output disabled. */
val SpeakerOffIcon: ImageVector by lazy {
    ImageVector.Builder(
        name = "SpeakerOff",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f,
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(3f, 9f)
            verticalLineTo(15f)
            horizontalLineTo(7f)
            lineTo(12f, 20f)
            verticalLineTo(4f)
            lineTo(7f, 9f)
            close()
            // Slash through the speaker
            moveTo(14f, 9.2f)
            lineTo(15.5f, 8f)
            lineTo(21f, 16.8f)
            lineTo(19.5f, 18f)
            close()
        }
    }.build()
}

@Composable
private fun ToolInvocationsCard(entry: ChatEntry.ToolInvocations, modifier: Modifier = Modifier) {
    var expanded by remember { mutableStateOf(false) }
    val invocations = entry.invocations
    val callingCount = invocations.count { it.toolResult == null }
    val completedCount = invocations.count { it.toolResult != null }
    val anyCalling = callingCount > 0
    val allSuccess = invocations.all { it.toolSuccess == true }
    val hasFailure = invocations.any { it.toolSuccess == false }

    val fg = MaterialTheme.colorScheme.onSurfaceVariant

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clickable { expanded = !expanded }
            .padding(horizontal = 16.dp, vertical = 6.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                text = if (expanded) "▼" else "▶",
                style = MaterialTheme.typography.labelSmall,
                color = fg,
            )
            if (anyCalling) {
                CircularProgressIndicator(
                    modifier = Modifier.size(16.dp),
                    strokeWidth = 2.dp,
                    color = fg,
                )
                Text(
                    text = stringResource(R.string.msg_tool_calling),
                    style = MaterialTheme.typography.labelLarge,
                    color = fg,
                )
                Text(
                    text = "($completedCount/${invocations.size})",
                    style = MaterialTheme.typography.labelSmall,
                    color = fg.copy(alpha = 0.6f),
                )
            } else {
                Text(
                    text = if (allSuccess) "✅" else if (hasFailure) "⚠️" else "✅",
                    style = MaterialTheme.typography.labelSmall,
                )
                Text(
                    text = stringResource(R.string.msg_tool_calls, invocations.size),
                    style = MaterialTheme.typography.labelLarge,
                    color = fg,
                )
            }
        }
        AnimatedVisibility(visible = expanded) {
            Column(modifier = Modifier.padding(top = 8.dp)) {
                for (inv in invocations) {
                    ToolCallRow(inv)
                }
            }
        }
    }
}

@Composable
private fun RichContentBlocks(
    presentation: ToolPresentation,
    onAction: ((String) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        presentation.blocks.forEach { block ->
            when (block) {
                is ContentBlock.Markdown -> MarkdownContent(block.text)
                is ContentBlock.SearchResults -> SearchResultsCard(block)
                is ContentBlock.Image -> ImageContentCard(block.media, block.alt)
                is ContentBlock.Audio -> AudioContentCard(block.media, block.title)
                is ContentBlock.Video -> VideoContentCard(block.media, block.title)
                is ContentBlock.Link -> LinkContentCard(block)
                is ContentBlock.Profile -> ProfileContentCard(block, onAction)
                is ContentBlock.Unsupported -> Unit
            }
        }
    }
}

@Composable
private fun ProfileContentCard(
    block: ContentBlock.Profile,
    onAction: ((String) -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(8.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        ),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(block.title, style = MaterialTheme.typography.titleSmall)
            Text(block.summary, style = MaterialTheme.typography.bodyMedium)
            block.items.take(8).forEach { item ->
                Column {
                    Text(
                        item.key,
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                    )
                    val before = item.before?.toString()?.take(160)
                    val after = item.after?.toString()?.take(160)
                    if (before != null) Text(before, style = MaterialTheme.typography.bodySmall)
                    if (after != null && after != before) {
                        Text("→ $after", style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
            if (block.items.size > 8) {
                Text("+${block.items.size - 8}", style = MaterialTheme.typography.labelSmall)
            }
            if (onAction != null && block.actions.isNotEmpty()) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    block.actions.forEach { action ->
                        if (action.destructive) {
                            Button(onClick = { onAction(action.command) }) { Text(action.label) }
                        } else {
                            TextButton(onClick = { onAction(action.command) }) { Text(action.label) }
                        }
                    }
                }
            }
        }
    }
}

private fun MediaReference.safeHttpsUrl(): String? = url?.takeIf { it.startsWith("https://") }

@Composable
private fun ImageContentCard(media: MediaReference, alt: String?, modifier: Modifier = Modifier) {
    val url = media.safeHttpsUrl()
    val imageLoader = rememberRichMediaImageLoader()
    var showPreview by remember(url) { mutableStateOf(false) }
    val colorScheme = MaterialTheme.colorScheme

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (url == null) {
            Text(
                text = "图片资源暂不可用",
                style = MaterialTheme.typography.bodySmall,
                color = colorScheme.onSurfaceVariant,
            )
        } else {
            AsyncImage(
                model = url,
                imageLoader = imageLoader,
                contentDescription = alt,
                contentScale = ContentScale.Crop,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 260.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .clickable { showPreview = true },
            )
            alt?.takeIf { it.isNotBlank() }?.let { description ->
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = colorScheme.onSurfaceVariant,
                )
            }
        }
    }

    if (showPreview && url != null) {
        Dialog(onDismissRequest = { showPreview = false }) {
            Image(
                painter = coil3.compose.rememberAsyncImagePainter(
                    model = url,
                    imageLoader = imageLoader,
                ),
                contentDescription = alt,
                contentScale = ContentScale.Fit,
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(1f),
            )
        }
    }
}

@Composable
private fun AudioContentCard(media: MediaReference, title: String?, modifier: Modifier = Modifier) {
    val url = media.safeHttpsUrl()
    val context = LocalContext.current
    if (url == null) {
        Text(
            text = "音频资源暂不可用",
            modifier = modifier.padding(top = 8.dp),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val player = remember(url) {
        ExoPlayer.Builder(context).build().apply { setMediaItem(MediaItem.fromUri(url)) }
    }
    var isPlaying by remember(player) { mutableStateOf(false) }
    var positionMs by remember(player) { mutableStateOf(0L) }
    var durationMs by remember(player) { mutableStateOf(media.durationMs ?: 0L) }
    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onIsPlayingChanged(playing: Boolean) { isPlaying = playing }
            override fun onPlaybackStateChanged(state: Int) {
                if (player.duration > 0) durationMs = player.duration
            }
        }
        player.addListener(listener)
        onDispose {
            player.removeListener(listener)
            player.release()
        }
    }
    LaunchedEffect(player, isPlaying) {
        while (isActive && isPlaying) {
            positionMs = player.currentPosition.coerceAtLeast(0L)
            delay(500)
        }
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 8.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        TextButton(onClick = {
            if (isPlaying) player.pause() else {
                player.prepare()
                player.play()
            }
        }) { Text(if (isPlaying) "暂停" else "播放") }
        Column(modifier = Modifier.weight(1f)) {
            Text(title ?: "音频", style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (durationMs > 0) {
                Text(
                    text = "${formatDuration(positionMs)} / ${formatDuration(durationMs)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun VideoContentCard(media: MediaReference, title: String?, modifier: Modifier = Modifier) {
    val url = media.safeHttpsUrl()
    val context = LocalContext.current
    if (url == null) {
        Text(
            text = "视频资源暂不可用",
            modifier = modifier.padding(top = 8.dp),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        return
    }
    val player = remember(url) {
        ExoPlayer.Builder(context).build().apply {
            setMediaItem(MediaItem.fromUri(url))
            prepare()
            playWhenReady = false
        }
    }
    DisposableEffect(player) { onDispose { player.release() } }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 8.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        title?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
        AndroidView(
            factory = { viewContext ->
                PlayerView(viewContext).apply {
                    this.player = player
                    useController = true
                }
            },
            update = { it.player = player },
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(16f / 9f)
                .clip(RoundedCornerShape(8.dp)),
        )
    }
}

@Composable
private fun LinkContentCard(block: ContentBlock.Link, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val safeUrl = block.url.takeIf { it.startsWith("https://") }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 8.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .then(if (safeUrl != null) Modifier.clickable {
                runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(safeUrl))) }
            } else Modifier)
            .padding(12.dp),
    ) {
        Text(block.title, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.primary)
        block.source?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
        block.description?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
    }
}

private fun formatDuration(durationMs: Long): String {
    val totalSeconds = (durationMs / 1000).coerceAtLeast(0)
    return "%d:%02d".format(totalSeconds / 60, totalSeconds % 60)
}

/** Compact renderer for structured search tool results. */
@Composable
private fun SearchResultsCard(block: ContentBlock.SearchResults, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val colorScheme = MaterialTheme.colorScheme

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 8.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = "搜索结果 · ${block.query}",
            style = MaterialTheme.typography.titleSmall,
            color = colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        block.items.take(3).forEach { item ->
            val isSafeExternalUrl = item.url.startsWith("https://")
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(8.dp))
                    .then(
                        if (isSafeExternalUrl) {
                            Modifier.clickable {
                                runCatching {
                                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(item.url)))
                                }
                            }
                        } else {
                            Modifier
                        },
                    )
                    .padding(vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalAlignment = Alignment.Top,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = item.title,
                        style = MaterialTheme.typography.bodyMedium,
                        color = colorScheme.primary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    item.source?.let { source ->
                        Text(
                            text = source,
                            style = MaterialTheme.typography.labelSmall,
                            color = colorScheme.onSurfaceVariant,
                        )
                    }
                    item.description?.let { description ->
                        Text(
                            text = description,
                            style = MaterialTheme.typography.bodySmall,
                            color = colorScheme.onSurfaceVariant,
                            maxLines = 3,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
        if (block.items.size > 3) {
            Text(
                text = "另有 ${block.items.size - 3} 条结果，可展开工具详情查看。",
                style = MaterialTheme.typography.labelSmall,
                color = colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun ToolCallRow(inv: ToolCallInfo, modifier: Modifier = Modifier) {
    var detailExpanded by remember { mutableStateOf(false) }
    val isCalling = inv.toolResult == null
    val isSuccess = inv.toolSuccess == true

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f))
            .clickable { detailExpanded = !detailExpanded }
            .padding(horizontal = 16.dp, vertical = 6.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                text = if (detailExpanded) "▼" else "▶",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (isCalling) {
                CircularProgressIndicator(
                    modifier = Modifier.size(12.dp),
                    strokeWidth = 1.5.dp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    text = inv.toolName,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    text = stringResource(R.string.msg_calling),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Text(
                    text = if (isSuccess) "✅" else "❌",
                    style = MaterialTheme.typography.labelSmall,
                )
                Text(
                    text = inv.toolName,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (!detailExpanded && inv.toolResult != null && inv.toolResult.length > 50) {
                    Text(
                        text = inv.toolResult.take(50) + "…",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        AnimatedVisibility(visible = detailExpanded) {
            Column(modifier = Modifier.padding(top = 6.dp)) {
                Text(
                    text = stringResource(R.string.msg_parameters),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    text = formatJson(inv.toolArgs),
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 2.dp),
                )
                if (inv.toolResult != null) {
                    Spacer(modifier = Modifier.height(6.dp))
                    Text(
                        text = stringResource(R.string.msg_result),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    SelectionContainer {
                        Text(
                            text = inv.toolResult,
                            style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 2.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ThinkingCard(content: String, modifier: Modifier = Modifier) {
    var expanded by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clickable { expanded = !expanded }
            .padding(horizontal = 16.dp, vertical = 6.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                text = if (expanded) "▼" else "▶",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = stringResource(R.string.msg_thinking_process),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        AnimatedVisibility(visible = expanded) {
            SelectionContainer {
                MarkdownContent(
                    content = content,
                    contentColor = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 8.dp),
                    cacheDocument = false,
                )
            }
        }
    }
}

@Composable
private fun SystemBubble(content: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
    ) {
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(999.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.7f))
                .padding(horizontal = 12.dp, vertical = 6.dp),
        ) {
            Text(
                text = content,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun DebugInfoCard(entry: ChatEntry.DebugInfo, modifier: Modifier = Modifier) {
    var expanded by remember { mutableStateOf(false) }
    val messages = remember(entry.messagesJson) { debugPromptMessages(entry.messagesJson) }
    var toolsExpanded by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.secondaryContainer)
            .clickable { expanded = !expanded }
            .padding(12.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                text = if (expanded) "▼" else "▶",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSecondaryContainer,
            )
            Text(
                text = stringResource(R.string.debug_prompt_token_estimates, entry.estimatedTokens, entry.estimatedToolTokens),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSecondaryContainer,
            )
        }
        AnimatedVisibility(visible = expanded) {
            SelectionContainer {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                    Text(
                        text = stringResource(R.string.debug_prompt_context_hint),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSecondaryContainer,
                    )
                    if (messages == null) {
                        Text(text = formatJson(entry.messagesJson), style = MaterialTheme.typography.bodySmall)
                    } else {
                        messages.forEach { message ->
                            val scope = stringResource(when (message.scope) {
                                DebugPromptScope.System -> R.string.debug_prompt_system
                                DebugPromptScope.History -> R.string.debug_prompt_history
                                DebugPromptScope.Current -> R.string.debug_prompt_current
                            })
                            Text(
                                text = "$scope · ${message.role}",
                                style = MaterialTheme.typography.labelLarge,
                                color = MaterialTheme.colorScheme.onSecondaryContainer,
                            )
                            Text(
                                text = message.json,
                                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                                color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.8f),
                            )
                        }
                    }
                    if (entry.toolsJson != null) {
                        TextButton(onClick = { toolsExpanded = !toolsExpanded }) {
                            Text(stringResource(R.string.debug_prompt_native_tools))
                        }
                        if (toolsExpanded) {
                            Text(
                                text = formatJson(entry.toolsJson),
                                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun formatJson(json: String): String {
    return try {
        val obj = org.json.JSONObject(json)
        obj.toString(2)
    } catch (_: Exception) {
        try {
            val arr = org.json.JSONArray(json)
            arr.toString(2)
        } catch (_: Exception) {
            json
        }
    }
}
