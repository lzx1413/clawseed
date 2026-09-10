package dev.clawseed.demo.ui.chat.markdown

import org.junit.Assert.*
import org.junit.Test

class MarkdownDocumentCacheTest {
    @Test fun completedMessagesAreReusedButStreamingDraftsDoNotEvictHistory() {
        val cache = MarkdownDocumentCache(maxCharacters = 100, maxEntries = 1)
        val history = cache.parse("**history**")
        repeat(20) { cache.parse("stream $it", cache = false) }
        assertSame(history, cache.parse("**history**"))
        assertEquals(parseMarkdown("# Heading"), cache.parse("# Heading"))
        assertNull(cache.get("**history**"))
    }

    @Test fun characterBudgetEvictsOldDocumentsAndOversizedMessagesAreNotRetained() {
        val cache = MarkdownDocumentCache(maxCharacters = 10)
        cache.parse("first")
        cache.parse("second")
        assertNull(cache.get("first"))
        val large = "x".repeat(11)
        assertEquals(parseMarkdown(large), cache.parse(large))
        assertNull(cache.get(large))
        assertNotNull(cache.get("second"))
    }
}
