package dev.clawseed.demo.ui.chat.markdown

import org.junit.Assert.assertEquals
import org.junit.Test

class MarkdownParserTest {

    @Test
    fun tableKeepsPipeInsideInlineCodeInOneCell() {
        val document = parseMarkdown(
            """
            | expression | result |
            | --- | --- |
            | `left | right` | ok |
            """.trimIndent(),
        )

        val table = document.blocks.single() as Table
        assertEquals(2, table.rows.single().size)
        assertEquals(listOf(InlineCode("left | right")), table.rows.single()[0])
        assertEquals(listOf(Text("ok")), table.rows.single()[1])
    }

    @Test
    fun tableNormalizesEveryBodyRowToHeaderWidth() {
        val document = parseMarkdown(
            """
            | first | second | third |
            | --- | --- | --- |
            | one | two |
            | alpha | beta | gamma | ignored |
            """.trimIndent(),
        )

        val table = document.blocks.single() as Table
        assertEquals(listOf(3, 3), table.rows.map { it.size })
        assertEquals(emptyList<InlineNode>(), table.rows[0][2])
        assertEquals(listOf(Text("gamma")), table.rows[1][2])
    }

    @Test
    fun tableKeepsEscapedPipeInOneCell() {
        val document = parseMarkdown(
            """
            | expression | result |
            | --- | --- |
            | left \| right | ok |
            """.trimIndent(),
        )

        val table = document.blocks.single() as Table
        assertEquals(listOf(Text("left | right")), table.rows.single()[0])
    }
}
