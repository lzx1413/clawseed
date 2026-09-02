package dev.clawseed.demo.ui.chat.markdown

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import dev.clawseed.demo.ui.chat.rememberRichMediaImageLoader

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
    if (block.inlines.none { it is Image }) {
        InlineContent(
            inlines = block.inlines,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(vertical = 2.dp),
        )
        return
    }

    val segments = mutableListOf<Any>()
    var textRun = mutableListOf<InlineNode>()
    fun flushTextRun() {
        if (textRun.hasVisibleContent()) {
            segments.add(textRun.toList())
        }
        textRun = mutableListOf()
    }

    for (inline in block.inlines) {
        if (inline is Image) {
            flushTextRun()
            segments.add(inline)
        } else {
            textRun.add(inline)
        }
    }
    flushTextRun()

    for (segment in segments) {
        when (segment) {
            is Image -> MarkdownImageBlock(segment)
            is List<*> -> InlineContent(
                inlines = segment.filterIsInstance<InlineNode>(),
                style = MaterialTheme.typography.bodyLarge,
                modifier = Modifier.padding(vertical = 2.dp),
            )
        }
    }
}

@Composable
private fun MarkdownImageBlock(image: Image) {
    val src = image.src.takeIf { it.startsWith("https://") }
    if (src == null) {
        Text(
            text = image.alt,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        )
        return
    }
    var failed by remember(src) { mutableStateOf(false) }

    if (failed) {
        Text(
            text = image.alt.ifBlank { src },
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        )
        Text(
            text = src,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp),
        )
        return
    }

    AsyncImage(
        model = src,
        imageLoader = rememberRichMediaImageLoader(),
        contentDescription = image.alt.takeIf { it.isNotBlank() },
        contentScale = ContentScale.Crop,
        onError = { failed = true },
        onSuccess = { failed = false },
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 120.dp, max = 260.dp)
            .padding(vertical = 4.dp)
            .clip(RoundedCornerShape(8.dp)),
    )
}

private fun List<InlineNode>.hasVisibleContent(): Boolean {
    return any { inline ->
        when (inline) {
            is Text -> inline.value.isNotBlank()
            LineBreak -> false
            else -> true
        }
    }
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
    Box(Modifier.fillMaxWidth().padding(vertical = 4.dp).horizontalScroll(rememberScrollState())) {
        Column {
            if (block.headers.any { it.isNotEmpty() }) {
                TableRow(block.headers, block.alignments, isHeader = true)
                HorizontalDivider()
            }
            for (row in block.rows) {
                TableRow(row, block.alignments, isHeader = false)
            }
        }
    }
}

@Composable
private fun TableRow(
    cells: List<List<InlineNode>>,
    alignments: List<ColumnAlign>,
    isHeader: Boolean,
) {
    Row {
        cells.forEachIndexed { index, cell ->
            InlineContent(
                inlines = cell,
                style = if (isHeader) {
                    MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Bold)
                } else {
                    MaterialTheme.typography.bodyLarge
                },
                textAlign = alignTextFor(alignments.getOrNull(index)),
                modifier = Modifier.width(TABLE_CELL_WIDTH).padding(4.dp),
            )
        }
    }
}

private val TABLE_CELL_WIDTH = 128.dp

private fun alignTextFor(align: ColumnAlign?): TextAlign = when (align) {
    ColumnAlign.LEFT -> TextAlign.Start
    ColumnAlign.CENTER -> TextAlign.Center
    ColumnAlign.RIGHT -> TextAlign.End
    else -> TextAlign.Unspecified
}
