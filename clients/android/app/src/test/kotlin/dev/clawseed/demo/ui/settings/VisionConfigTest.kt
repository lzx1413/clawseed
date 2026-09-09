package dev.clawseed.demo.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Test

class VisionConfigTest {
    @Test fun visionReadsOnlyTheActiveProfileAndDefaultsToAuto() {
        val base = """
            [providers]
            fallback = "active"
            [providers.models."other"]
            vision = "enabled"
            [providers.models."active"]
            model = "my-model"
        """.trimIndent()
        assertEquals("auto", SettingsViewModel.extractProviderVision(base))
        for (mode in listOf("auto", "enabled", "disabled")) {
            assertEquals(mode, SettingsViewModel.extractProviderVision("$base\nvision = \"$mode\""))
        }
    }
}
