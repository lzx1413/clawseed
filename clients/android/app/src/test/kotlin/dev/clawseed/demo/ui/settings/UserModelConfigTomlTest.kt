package dev.clawseed.demo.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UserModelConfigTomlTest {
    @Test
    fun extractsOnlyEnabledAutomaticInference() {
        assertFalse(UserModelConfigToml.extractAutoInfer("[agent]\nmax_tokens = 100"))
        assertFalse(
            UserModelConfigToml.extractAutoInfer(
                "[user_model]\nenabled = false\nauto_infer = true\n",
            ),
        )
        assertTrue(
            UserModelConfigToml.extractAutoInfer(
                "[user_model]\nenabled = true\nauto_infer = true # user choice\n",
            ),
        )
    }

    @Test
    fun updatesOnlyAutoInferAndPreservesOtherSections() {
        val original = """
            [user_model]
            enabled = true
            max_prompt_items = 7
            auto_infer = false # opt in
            inference_min_confidence = 0.9

            [gateway]
            port = 42617
        """.trimIndent() + "\n"

        val updated = UserModelConfigToml.updateAutoInfer(original, true)

        assertTrue(updated.contains("auto_infer = true # opt in"))
        assertTrue(updated.contains("max_prompt_items = 7"))
        assertTrue(updated.contains("inference_min_confidence = 0.9"))
        assertTrue(updated.contains("[gateway]\nport = 42617"))
        assertTrue(updated.endsWith('\n'))
    }

    @Test
    fun insertsMissingKeyIntoExistingSection() {
        val original = "[user_model]\nenabled = true\n\n[gateway]\nport = 42617"
        val updated = UserModelConfigToml.updateAutoInfer(original, true)

        assertEquals(
            "[user_model]\nenabled = true\nauto_infer = true\n\n[gateway]\nport = 42617",
            updated,
        )
    }

    @Test
    fun appendsCompleteSectionWhenMissing() {
        val updated = UserModelConfigToml.updateAutoInfer("[gateway]\nport = 42617\n", false)

        assertTrue(updated.contains("[user_model]\nenabled = true\n"))
        assertTrue(updated.contains("auto_infer = false\n"))
        assertTrue(updated.contains("inference_min_confidence = 0.8\n"))
        assertTrue(updated.endsWith('\n'))
    }

    @Test
    fun enablingInferenceAlsoEnablesUserModeling() {
        val original = "[user_model]\nenabled = false\nauto_infer = false\n"

        val updated = UserModelConfigToml.updateAutoInfer(original, true)

        assertTrue(updated.contains("enabled = true"))
        assertTrue(UserModelConfigToml.extractAutoInfer(updated))
    }
}
