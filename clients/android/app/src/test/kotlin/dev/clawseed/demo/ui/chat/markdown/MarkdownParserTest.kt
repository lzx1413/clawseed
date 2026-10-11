package dev.clawseed.demo.ui.chat.markdown

import org.junit.Assert.assertEquals
import org.junit.Test

class MarkdownParserTest {

    @Test
    fun visualizeFenceParsesSupportedChartConfig() {
        val document = parseMarkdown(
            """
            ```visualize
            {"type":"line","title":"Visits","labels":["Mon","Tue"],"datasets":[{"label":"Visits","data":[12,18],"color":"#4ecca3"}]}
            ```
            """.trimIndent(),
        )

        val chart = document.blocks.single() as Visualization
        assertEquals("line", chart.config?.type)
        assertEquals(listOf("Mon", "Tue"), chart.config?.labels)
        assertEquals(listOf(12.0, 18.0), chart.config?.datasets?.single()?.data)
    }

    @Test
    fun invalidOrIncompleteVisualizeFenceRemainsSafe() {
        val invalid = parseMarkdown("```visualize\n{\"type\":\"radar\"}\n```").blocks.single() as Visualization
        assertEquals(null, invalid.config)

        val incomplete = parseMarkdown("```visualize\n{\"type\":\"line\"").blocks.single() as Visualization
        assertEquals(false, incomplete.closed)
        assertEquals(null, incomplete.config)
    }

    @Test
    fun dashboardFenceParsesMultipleCharts() {
        val document = parseMarkdown(
            """
            ```visualize
            {"type":"dashboard","title":"Overview","charts":[{"type":"line","title":"Trend","labels":["A","B"],"datasets":[{"label":"Count","data":[1,2]}]},{"type":"pie","title":"Share","labels":["X","Y"],"datasets":[{"label":"Share","data":[3,7]}]}]}
            ```
            """.trimIndent(),
        )

        val dashboard = document.blocks.single() as Visualization
        assertEquals("dashboard", dashboard.config?.type)
        assertEquals(listOf("Trend", "Share"), dashboard.config?.charts?.map { it.title })
    }

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
