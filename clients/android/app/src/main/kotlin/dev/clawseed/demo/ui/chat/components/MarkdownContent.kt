package dev.clawseed.demo.ui.chat.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.MaterialTheme
import dev.clawseed.demo.ui.chat.markdown.MarkdownContent
import dev.clawseed.demo.ui.chat.markdown.parseMarkdown

/**
 * Public API for rendering markdown content in chat messages.
 *
 * Thin wrapper that delegates parsing and rendering to the [markdown] package.
 * The [contentColor] parameter allows callers (e.g. ThinkingCard) to override
 * the default onSurface color.
 */
@Composable
fun MarkdownContent(
    content: String,
    modifier: Modifier = Modifier,
    contentColor: Color = MaterialTheme.colorScheme.onSurface,
) {
    val document = remember(content) { parseMarkdown(content) }
    MarkdownContent(document = document, modifier = modifier, contentColor = contentColor)
}
