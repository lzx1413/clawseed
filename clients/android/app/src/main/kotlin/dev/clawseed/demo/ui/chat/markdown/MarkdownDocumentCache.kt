package dev.clawseed.demo.ui.chat.markdown

/** Bounded cache shared by message rows that leave and re-enter the lazy list. */
internal class MarkdownDocumentCache(
    private val maxCharacters: Int = 256 * 1024,
    private val maxEntries: Int = 64,
) {
    private val documents = LinkedHashMap<String, MarkdownDocument>(16, 0.75f, true)
    private var characters = 0

    @Synchronized
    fun get(content: String): MarkdownDocument? = documents[content]

    fun parse(content: String, cache: Boolean = true): MarkdownDocument {
        if (cache) get(content)?.let { return it }
        val document = parseMarkdown(content)
        if (cache && content.length <= maxCharacters) synchronized(this) {
            if (documents.put(content, document) == null) characters += content.length
            while (characters > maxCharacters || documents.size > maxEntries) {
                val oldest = documents.entries.iterator()
                characters -= oldest.next().key.length
                oldest.remove()
            }
        }
        return document
    }
}

internal val chatMarkdownCache = MarkdownDocumentCache()
