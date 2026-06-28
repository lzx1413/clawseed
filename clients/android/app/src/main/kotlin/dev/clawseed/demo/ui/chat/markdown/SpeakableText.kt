package dev.clawseed.demo.ui.chat.markdown

/**
 * Markdown-aware, TTS-friendly text for a message: formatting is stripped, code blocks and
 * horizontal rules are dropped, and the human-readable text of paragraphs, headings, lists,
 * tables and blockquotes is concatenated so the user hears what the reply says — not the
 * markdown behind it. Mirrors Kai's `String.toSpeakableText()` in `KaiUiTts.kt`.
 *
 * Inline code is kept (it is usually short and readable); image alt text is skipped.
 */
fun String.toSpeakableText(): String {
    val doc = parseMarkdown(this)
    val parts = mutableListOf<String>()
    for (block in doc.blocks) {
        block.collectSpeakableText(parts)
    }
    return parts.joinToString("\n")
        .replace(Regex("[ \\t]+"), " ")
        .trim()
}

private fun BlockNode.collectSpeakableText(parts: MutableList<String>) {
    when (this) {
        is Heading -> parts += inlines.toSpeakable()
        is Paragraph -> parts += inlines.toSpeakable()
        is Blockquote -> children.forEach { it.collectSpeakableText(parts) }
        is BulletList -> items.forEach { it.collectItem(parts) }
        is OrderedList -> items.forEach { it.collectItem(parts) }
        is Table -> {
            headers.forEach { col -> parts += col.toSpeakable() }
            rows.forEach { row -> row.forEach { col -> parts += col.toSpeakable() } }
        }
        // Code blocks and rules carry no value when read aloud.
        is CodeFence -> Unit
        is HorizontalRule -> Unit
    }
}

private fun ListItem.collectItem(parts: MutableList<String>) {
    val itemParts = mutableListOf<String>()
    children.forEach { it.collectSpeakableText(itemParts) }
    val joined = itemParts.joinToString("\n").trim()
    if (joined.isNotEmpty()) parts += joined
}

private fun List<InlineNode>.toSpeakable(): String =
    joinToString("") { it.toSpeakable() }.trim()

private fun InlineNode.toSpeakable(): String =
    when (this) {
        is Text -> value
        is Emphasis -> children.toSpeakable()
        is Strong -> children.toSpeakable()
        is Strike -> children.toSpeakable()
        is InlineCode -> code
        is Link -> children.toSpeakable()
        // Images contribute nothing useful when spoken; skip the alt text.
        is Image -> ""
        is LineBreak -> "\n"
    }
