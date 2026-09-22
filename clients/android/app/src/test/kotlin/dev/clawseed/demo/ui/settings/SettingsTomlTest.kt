package dev.clawseed.demo.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.tomlj.Toml

class SettingsTomlTest {
    @Test
    fun replacingMultilineProviderModelsArrayRemovesOldContinuationLines() {
        val initial = """
            [providers.models.mimo]
            models = [
                "mimo-v2.5",
                "mimo-v2.5-asr",
                "mimo-v2.5-pro",
            ]

            [agent]
            context_compaction_enabled = true
        """.trimIndent()

        assertEquals(
            listOf("mimo-v2.5", "mimo-v2.5-asr", "mimo-v2.5-pro"),
            SettingsViewModel.parseTomlArray(initial, "[providers.models.mimo]", "models"),
        )

        val updated = SettingsViewModel.updateTomlArray(
            initial,
            "[providers.models.mimo]",
            "models",
            listOf("mimo-v2.5-pro", "mimo-v2.5-mini"),
        )
        val parsed = Toml.parse(updated)

        assertFalse(parsed.errors().joinToString("\n"), parsed.hasErrors())
        assertEquals(
            listOf("mimo-v2.5-pro", "mimo-v2.5-mini"),
            parsed.getArray("providers.models.mimo.models")!!.toList(),
        )
    }
}
