package dev.clawseed.demo.ui.chat.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.MaterialTheme
import dev.clawseed.demo.ui.chat.markdown.MarkdownContent
import dev.clawseed.demo.ui.chat.markdown.chatMarkdownCache
import dev.clawseed.demo.ui.chat.markdown.MarkdownDocument
import dev.clawseed.demo.ui.chat.markdown.Paragraph
import dev.clawseed.demo.ui.chat.markdown.Text
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.conflate

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
    cacheDocument: Boolean = true,
) {
    val cached = remember(content, cacheDocument) {
        if (cacheDocument) chatMarkdownCache.get(content) else null
    }
    val latestContent by rememberUpdatedState(content)
    val parsed by produceState(cached, cacheDocument) {
        // Keep one parser per row. If parsing is slower than the stream, consume
        // the latest snapshot next instead of accumulating cancelled CPU work.
        snapshotFlow { latestContent }.conflate().collect { text ->
            value = (if (cacheDocument) chatMarkdownCache.get(text) else null)
                ?: withContext(Dispatchers.Default) { chatMarkdownCache.parse(text, cacheDocument) }
        }
    }
    // Keep the previous formatted stream while its next snapshot is parsed.
    // A new row can show its text immediately, without parsing on the UI thread.
    val document = cached ?: parsed ?: remember(content) {
        MarkdownDocument(listOf(Paragraph(listOf(Text(content)))))
    }
    MarkdownContent(document = document, modifier = modifier, contentColor = contentColor)
}
