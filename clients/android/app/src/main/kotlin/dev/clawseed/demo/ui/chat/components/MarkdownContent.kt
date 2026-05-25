package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager as LocalClip
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDirection
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp

@Composable
fun MarkdownContent(
    content: String,
    modifier: Modifier = Modifier,
    contentColor: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.onSurface,
) {
    val blocks = remember(content) { parseBlocks(content) }
    Column(modifier = modifier) {
        for (block in blocks) {
            when (block) {
                is MdBlock.CodeBlock -> CodeBlock(
                    code = block.code,
                    language = block.language,
                    contentColor = contentColor,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
                is MdBlock.Paragraph -> {
                    val styled = remember(block.text) { parseInlineMarkdown(block.text) }
                    Text(
                        text = styled,
                        style = MaterialTheme.typography.bodyLarge.copy(textDirection = TextDirection.Ltr),
                        color = contentColor,
                    )
                }
                is MdBlock.Heading -> {
                    val style = when (block.level) {
                        1 -> MaterialTheme.typography.headlineSmall
                        2 -> MaterialTheme.typography.titleLarge
                        else -> MaterialTheme.typography.titleMedium
                    }
                    val styled = remember(block.text) { parseInlineMarkdown(block.text) }
                    Text(text = styled, style = style.copy(textDirection = TextDirection.Ltr), color = contentColor)
                }
                is MdBlock.ListItem -> {
                    val styled = remember(block.text) { parseInlineMarkdown(block.text) }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.Top,
                    ) {
                        Text(
                            text = "•",
                            style = MaterialTheme.typography.bodyLarge,
                            color = contentColor,
                        )
                        Text(
                            text = styled,
                            style = MaterialTheme.typography.bodyLarge.copy(textDirection = TextDirection.Ltr),
                            color = contentColor,
                            modifier = Modifier.weight(1f),
                        )
                    }
                }
                is MdBlock.Table -> TableBlock(
                    headers = block.headers,
                    rows = block.rows,
                    contentColor = contentColor,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
            }
        }
    }
}

@Composable
fun CodeBlock(
    code: String,
    language: String?,
    modifier: Modifier = Modifier,
    contentColor: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.onSurface,
) {
    val clipboardManager = LocalClip.current
    var copied by remember { mutableStateOf(false) }

    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 8.dp, topEnd = 8.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.7f))
                .padding(horizontal = 12.dp, vertical = 4.dp),
            horizontalArrangement = androidx.compose.foundation.layout.Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = language ?: "code",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(
                onClick = {
                    clipboardManager.setText(AnnotatedString(code))
                    copied = true
                },
                contentPadding = androidx.compose.foundation.layout.PaddingValues(),
            ) {
                Text(
                    text = if (copied) "已复制" else "复制",
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(bottomStart = 8.dp, bottomEnd = 8.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant)
                .padding(12.dp),
        ) {
            Text(
                text = code,
                style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                color = contentColor,
            )
        }
    }
}

private sealed class MdBlock {
    data class Paragraph(val text: String) : MdBlock()
    data class Heading(val level: Int, val text: String) : MdBlock()
    data class CodeBlock(val code: String, val language: String?) : MdBlock()
    data class ListItem(val text: String) : MdBlock()
    data class Table(val headers: List<String>, val rows: List<List<String>>) : MdBlock()
}

private fun parseBlocks(content: String): List<MdBlock> {
    val blocks = mutableListOf<MdBlock>()
    val lines = content.lines()
    var i = 0
    while (i < lines.size) {
        val line = lines[i]
        if (line.trimStart().startsWith("```")) {
            val lang = line.trimStart().removePrefix("```").trim().takeIf { it.isNotEmpty() }
            val codeLines = mutableListOf<String>()
            i++
            while (i < lines.size && !lines[i].trimStart().startsWith("```")) {
                codeLines.add(lines[i])
                i++
            }
            blocks.add(MdBlock.CodeBlock(codeLines.joinToString("\n"), lang))
            i++
            continue
        }
        val headingMatch = Regex("^(#{1,6})\\s+(.+)").find(line)
        if (headingMatch != null) {
            blocks.add(MdBlock.Heading(headingMatch.groupValues[1].length, headingMatch.groupValues[2]))
            i++
            continue
        }
        if (line.trimStart().startsWith("- ") || line.trimStart().startsWith("* ") || Regex("^\\d+\\.\\s").containsMatchIn(line.trimStart())) {
            val stripped = line.trimStart()
                .removePrefix("- ").removePrefix("* ")
                .let { Regex("^\\d+\\.\\s+").replace(it, "") }
            blocks.add(MdBlock.ListItem(stripped))
            i++
            continue
        }
        if (line.contains('|') && i + 1 < lines.size) {
            val sepLine = lines[i + 1].trim()
            if (sepLine.matches(Regex("^\\|?[\\s:]*-{2,}[\\s:]*\\|.*"))) {
                val headers = parseTableRow(line)
                i += 2
                val rows = mutableListOf<List<String>>()
                while (i < lines.size && lines[i].contains('|')) {
                    val cells = parseTableRow(lines[i])
                    if (cells.isNotEmpty()) rows.add(cells)
                    i++
                }
                blocks.add(MdBlock.Table(headers, rows))
                continue
            }
        }
        if (line.isNotBlank()) {
            blocks.add(MdBlock.Paragraph(line))
        }
        i++
    }
    return blocks
}

private fun parseInlineMarkdown(text: String): AnnotatedString = buildAnnotatedString {
    val regex = Regex("""(\*\*|__)(.+?)\1|(\*|_)(.+?)\3|`(.+?)`""")
    var lastEnd = 0
    for (match in regex.findAll(text)) {
        append(text.substring(lastEnd, match.range.first))
        when {
            match.groupValues[2].isNotEmpty() -> withStyle(SpanStyle(fontWeight = FontWeight.Bold)) {
                append(match.groupValues[2])
            }
            match.groupValues[4].isNotEmpty() -> withStyle(SpanStyle(fontStyle = androidx.compose.ui.text.font.FontStyle.Italic)) {
                append(match.groupValues[4])
            }
            match.groupValues[5].isNotEmpty() -> withStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = androidx.compose.ui.graphics.Color.Transparent)) {
                append(match.groupValues[5])
            }
        }
        lastEnd = match.range.last + 1
    }
    if (lastEnd < text.length) append(text.substring(lastEnd))
}

private fun parseTableRow(line: String): List<String> {
    val trimmed = line.trim().removePrefix("|").removeSuffix("|")
    return trimmed.split("|").map { it.trim() }
}

@Composable
private fun TableBlock(
    headers: List<String>,
    rows: List<List<String>>,
    modifier: Modifier = Modifier,
    contentColor: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.onSurface,
) {
    val colCount = headers.size
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f)),
    ) {
        Row(
            modifier = Modifier
                .height(IntrinsicSize.Min)
                .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.7f)),
        ) {
            for ((ci, header) in headers.withIndex()) {
                val styled = remember(header) { parseInlineMarkdown(header) }
                TableCell(
                    text = styled,
                    bold = true,
                    showDivider = ci < colCount - 1,
                )
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        for ((ri, row) in rows.withIndex()) {
            Row(
                modifier = Modifier
                    .height(IntrinsicSize.Min)
                    .then(
                        if (ri % 2 == 1) Modifier.background(
                            MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.15f)
                        ) else Modifier
                    ),
            ) {
                for (ci in 0 until colCount) {
                    val cell = row.getOrElse(ci) { "" }
                    val styled = remember(cell) { parseInlineMarkdown(cell) }
                    TableCell(
                        text = styled,
                        bold = false,
                        showDivider = ci < colCount - 1,
                    )
                }
            }
            if (ri < rows.lastIndex) {
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
            }
        }
    }
}

@Composable
private fun RowScope.TableCell(
    text: AnnotatedString,
    bold: Boolean,
    showDivider: Boolean,
    contentColor: androidx.compose.ui.graphics.Color = MaterialTheme.colorScheme.onSurface,
) {
    Box(
        modifier = Modifier
            .weight(1f)
            .fillMaxHeight()
            .padding(horizontal = 8.dp, vertical = 6.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.bodyMedium.copy(
                fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal,
                textDirection = TextDirection.Ltr,
            ),
            color = contentColor,
        )
    }
    if (showDivider) {
        Box(
            modifier = Modifier
                .fillMaxHeight()
                .width(1.dp)
                .background(MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f)),
        )
    }
}
