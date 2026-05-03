package dev.clawseed.demo.ui.chat.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.data.ChatEntry

@Composable
fun MessageBubble(
    entry: ChatEntry,
    modifier: Modifier = Modifier,
) {
    when (entry) {
        is ChatEntry.UserMessage -> UserBubble(entry.content, modifier)
        is ChatEntry.AssistantMessage -> AssistantBubble(entry.content, entry.isStreaming, modifier)
        is ChatEntry.ToolCall -> ToolCallCard(entry, modifier)
        is ChatEntry.ToolResult -> ToolResultCard(entry, modifier)
        is ChatEntry.Thinking -> ThinkingCard(entry.content, modifier)
        is ChatEntry.DebugInfo -> DebugInfoCard(entry, modifier)
    }
}

@Composable
private fun UserBubble(content: String, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.End,
    ) {
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(16.dp, 16.dp, 4.dp, 16.dp))
                .background(MaterialTheme.colorScheme.primary)
                .padding(horizontal = 16.dp, vertical = 10.dp),
        ) {
            Text(
                text = content,
                color = MaterialTheme.colorScheme.onPrimary,
                style = MaterialTheme.typography.bodyLarge,
            )
        }
    }
}

@Composable
private fun AssistantBubble(content: String, isStreaming: Boolean, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Start,
    ) {
        SelectionContainer {
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(16.dp, 16.dp, 16.dp, 4.dp))
                    .background(MaterialTheme.colorScheme.surfaceVariant)
                    .padding(horizontal = 16.dp, vertical = 10.dp),
            ) {
                Column {
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
        }
    }
}

@Composable
private fun ToolCallCard(entry: ChatEntry.ToolCall, modifier: Modifier = Modifier) {
    var expanded by remember { mutableStateOf(false) }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.tertiaryContainer)
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
                color = MaterialTheme.colorScheme.onTertiaryContainer,
            )
            Text(
                text = entry.toolName,
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onTertiaryContainer,
            )
        }
        AnimatedVisibility(visible = expanded) {
            Text(
                text = formatJson(entry.toolArgs),
                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.onTertiaryContainer.copy(alpha = 0.8f),
                modifier = Modifier.padding(top = 8.dp),
            )
        }
    }
}

@Composable
private fun ToolResultCard(entry: ChatEntry.ToolResult, modifier: Modifier = Modifier) {
    var expanded by remember { mutableStateOf(false) }
    val bg = if (entry.toolSuccess) MaterialTheme.colorScheme.tertiaryContainer
             else MaterialTheme.colorScheme.errorContainer
    val fg = if (entry.toolSuccess) MaterialTheme.colorScheme.onTertiaryContainer
             else MaterialTheme.colorScheme.onErrorContainer

    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(bg)
            .clickable { expanded = !expanded }
            .padding(12.dp),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                text = if (entry.toolSuccess) "✅" else "❌",
                style = MaterialTheme.typography.labelSmall,
            )
            Text(
                text = entry.toolName,
                style = MaterialTheme.typography.labelLarge,
                color = fg,
            )
            if (!expanded && entry.toolResult.length > 60) {
                Text(
                    text = entry.toolResult.take(60) + "…",
                    style = MaterialTheme.typography.bodySmall,
                    color = fg.copy(alpha = 0.6f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        AnimatedVisibility(visible = expanded) {
            SelectionContainer {
                Text(
                    text = entry.toolResult,
                    style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                    color = fg.copy(alpha = 0.8f),
                    modifier = Modifier.padding(top = 8.dp),
                )
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
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
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
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = "​思考过程",
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        AnimatedVisibility(visible = expanded) {
            Text(
                text = content,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.8f),
                modifier = Modifier.padding(top = 8.dp),
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
