package dev.clawseed.demo.ui.chat.markdown

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/**
 * Render a parsed [MarkdownDocument] as a Compose layout. Each block becomes one child of the
 * outer [Column]; inline content is rendered as [androidx.compose.ui.text.AnnotatedString].
 */
@Composable
fun MarkdownContent(
    document: MarkdownDocument,
    modifier: Modifier = Modifier,
    contentColor: Color = MaterialTheme.colorScheme.onSurface,
) {
    CompositionLocalProvider(LocalContentColor provides contentColor) {
        Column(modifier) {
            for (block in document.blocks) {
                BlockRenderer(block)
            }
        }
    }
}

@Composable
private fun BlockRenderer(block: BlockNode) {
    when (block) {
        is Heading -> HeadingBlock(block)

        is Paragraph -> ParagraphBlock(block)

        is CodeFence -> {
            if (block.code.isNotBlank() || !block.language.isNullOrBlank()) {
                CodeFenceBlock(
                    language = block.language,
                    code = block.code,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
        }

        is Blockquote -> BlockquoteBlock(block)

        is BulletList -> BulletListBlock(block)

        is OrderedList -> OrderedListBlock(block)

        is Table -> TableBlock(block)

        HorizontalRule -> HorizontalDivider(Modifier.padding(vertical = 8.dp))
    }
}

@Composable
private fun HeadingBlock(block: Heading) {
    val typography = MaterialTheme.typography
    val style = when (block.level) {
        1 -> typography.headlineSmall
        2 -> typography.titleLarge
        3 -> typography.titleMedium
        4 -> typography.titleSmall
        5 -> typography.bodyLarge.copy(fontWeight = FontWeight.Bold)
        else -> typography.bodyMedium.copy(fontWeight = FontWeight.Bold)
    }
    InlineContent(
        inlines = block.inlines,
        style = style,
        modifier = Modifier.padding(vertical = 4.dp),
    )
}

@Composable
private fun ParagraphBlock(block: Paragraph) {
    if (block.inlines.size == 1 && block.inlines[0] is Image) {
        // Image-only paragraph: show alt text as fallback (no Coil dependency)
        val img = block.inlines[0] as Image
        Text(
            text = img.alt,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        )
        return
    }
    InlineContent(
        inlines = block.inlines,
        style = MaterialTheme.typography.bodyLarge,
        modifier = Modifier.padding(vertical = 2.dp),
    )
}

@Composable
private fun BlockquoteBlock(block: Blockquote) {
    Row(modifier = Modifier.padding(vertical = 4.dp).height(IntrinsicSize.Min)) {
        VerticalDivider(
            thickness = 3.dp,
            color = MaterialTheme.colorScheme.outline,
            modifier = Modifier.fillMaxHeight(),
        )
        Column(Modifier.padding(start = 8.dp)) {
            block.children.forEach { BlockRenderer(it) }
        }
    }
}

@Composable
private fun BulletListBlock(block: BulletList) {
    Column(modifier = Modifier.padding(vertical = 2.dp)) {
        for (item in block.items) {
            ListItemRow("•", 16.dp, item)
        }
    }
}

@Composable
private fun OrderedListBlock(block: OrderedList) {
    Column(modifier = Modifier.padding(vertical = 2.dp)) {
        block.items.forEachIndexed { index, item ->
            ListItemRow("${block.start + index}.", 24.dp, item)
        }
    }
}

@Composable
private fun ListItemRow(
    marker: String,
    markerWidth: androidx.compose.ui.unit.Dp,
    item: ListItem,
) {
    Row {
        Text(
            text = marker,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.width(markerWidth).padding(end = 4.dp),
        )
        Column(Modifier.fillMaxWidth()) {
            item.children.forEach { BlockRenderer(it) }
        }
    }
}

@Composable
private fun TableBlock(block: Table) {
    Column(Modifier.padding(vertical = 4.dp)) {
        if (block.headers.any { it.isNotEmpty() }) {
            Row {
                block.headers.forEachIndexed { i, cell ->
                    InlineContent(
                        inlines = cell,
                        style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Bold),
                        textAlign = alignTextFor(block.alignments.getOrNull(i)),
                        modifier = Modifier.weight(1f).padding(4.dp),
                    )
                }
            }
            HorizontalDivider()
        }
        for (row in block.rows) {
            Row {
                row.forEachIndexed { i, cell ->
                    InlineContent(
                        inlines = cell,
                        style = MaterialTheme.typography.bodyLarge,
                        textAlign = alignTextFor(block.alignments.getOrNull(i)),
                        modifier = Modifier.weight(1f).padding(4.dp),
                    )
                }
            }
        }
    }
}

private fun alignTextFor(align: ColumnAlign?): TextAlign = when (align) {
    ColumnAlign.LEFT -> TextAlign.Start
    ColumnAlign.CENTER -> TextAlign.Center
    ColumnAlign.RIGHT -> TextAlign.End
    else -> TextAlign.Unspecified
}
