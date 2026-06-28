package dev.clawseed.demo.ui.chat.components

import android.widget.Toast
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
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
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.data.ChatEntry
import dev.clawseed.demo.data.ToolCallInfo
import kotlinx.coroutines.delay

@Composable
fun MessageBubble(
    entry: ChatEntry,
    modifier: Modifier = Modifier,
    onRegenerate: (() -> Unit)? = null,
    onSpeak: ((String) -> Unit)? = null,
    onStop: (() -> Unit)? = null,
    isSpeakingThis: Boolean = false,
) {
    when (entry) {
        is ChatEntry.UserMessage -> UserBubble(entry.content, modifier)
        is ChatEntry.AssistantMessage -> AssistantBubble(
            content = entry.content,
            isStreaming = entry.isStreaming,
            onRegenerate = onRegenerate,
            onSpeak = onSpeak,
            onStop = onStop,
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
                .background(MaterialTheme.colorScheme.primary)
                .padding(horizontal = 16.dp, vertical = 10.dp),
        ) {
            val selectionColors = TextSelectionColors(
                handleColor = MaterialTheme.colorScheme.onPrimary,
                backgroundColor = MaterialTheme.colorScheme.onPrimary.copy(alpha = 0.3f),
            )
            CompositionLocalProvider(LocalTextSelectionColors provides selectionColors) {
                SelectionContainer {
                    Text(
                        text = content,
                        color = MaterialTheme.colorScheme.onPrimary,
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
    isStreaming: Boolean,
    onRegenerate: (() -> Unit)?,
    onSpeak: ((String) -> Unit)?,
    onStop: (() -> Unit)?,
    isSpeakingThis: Boolean,
    modifier: Modifier = Modifier,
) {
    val clipboardManager = LocalClipboardManager.current
    val context = LocalContext.current
    val copiedText = stringResource(R.string.common_copied)

    Column(modifier = modifier.fillMaxWidth()) {
        SelectionContainer {
            Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                MarkdownContent(content = content)
                if (isStreaming) {
                    Text(
                        text = "█",
                        color = MaterialTheme.colorScheme.primary,
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
            }
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
                   else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
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
            tint = if (copied) MaterialTheme.colorScheme.primary
                   else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
        )
        if (copied) {
            Text(
                text = stringResource(R.string.msg_copied),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
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
            tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
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
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
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
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
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
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
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
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                )
                Text(
                    text = formatJson(inv.toolArgs),
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                    modifier = Modifier.padding(top = 2.dp),
                )
                if (inv.toolResult != null) {
                    Spacer(modifier = Modifier.height(6.dp))
                    Text(
                        text = stringResource(R.string.msg_result),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                    )
                    SelectionContainer {
                        Text(
                            text = inv.toolResult,
                            style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
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
                    contentColor = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.8f),
                    modifier = Modifier.padding(top = 8.dp),
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
                text = "Debug: ~${entry.estimatedTokens} tokens",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSecondaryContainer,
            )
        }
        AnimatedVisibility(visible = expanded) {
            SelectionContainer {
                Text(
                    text = formatJson(entry.messagesJson),
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                    color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.8f),
                    modifier = Modifier.padding(top = 8.dp),
                )
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
