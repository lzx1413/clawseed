package dev.clawseed.demo.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.luminance
import dev.clawseed.demo.ui.persona.personaInitialColor
import dev.clawseed.demo.ui.persona.readablePersonaColor
import org.junit.Assert.*
import org.junit.Test

class AppColorsTest {
    private val schemes = listOf(appColorScheme(false), appColorScheme(true), appColorScheme(true, true))

    private fun contrast(foreground: Color, background: Color): Float {
        val a = foreground.compositeOver(background).luminance()
        val b = background.luminance()
        return (maxOf(a, b) + 0.05f) / (minOf(a, b) + 0.05f)
    }

    @Test fun bodyAndSecondaryTextRemainReadableOnEveryNeutralSurface() {
        schemes.forEach { scheme ->
            val surfaces = listOf(scheme.background, scheme.surface, scheme.surfaceVariant,
                scheme.surfaceContainerLow, scheme.surfaceContainer, scheme.surfaceContainerHigh,
                scheme.surfaceContainerHighest)
            for (surface in surfaces) {
                assertTrue("Body text on $surface", contrast(scheme.onSurface, surface) >= 4.5f)
                assertTrue("Secondary text on $surface", contrast(scheme.onSurfaceVariant, surface) >= 4.5f)
            }
        }
    }

    @Test fun actionsSelectionsAndStatusContainersHaveReadableText() {
        schemes.forEach { scheme ->
            val pairs = listOf(
                scheme.onPrimary to scheme.primary,
                scheme.onPrimaryContainer to scheme.primaryContainer,
                scheme.onSecondaryContainer to scheme.secondaryContainer,
                scheme.onTertiaryContainer to scheme.tertiaryContainer,
                scheme.onErrorContainer to scheme.errorContainer,
                scheme.inverseOnSurface to scheme.inverseSurface,
            )
            pairs.forEach { (text, surface) -> assertTrue(contrast(text, surface) >= 4.5f) }
        }
    }

    @Test fun successAndErrorAreReadableAndDistinctFromSelection() {
        schemes.forEach { scheme ->
            assertNotEquals(scheme.primary, scheme.success)
            assertNotEquals(scheme.error, scheme.success)
            for (background in listOf(scheme.surface, scheme.surfaceVariant, scheme.surfaceContainerHigh)) {
                assertTrue(contrast(scheme.success, background) >= 4.5f)
                assertTrue(contrast(scheme.error, background) >= 4.5f)
            }
        }
    }

    @Test fun oledChangesTheCanvasWithoutLosingReadableSurfaceLayers() {
        val dark = appColorScheme(true)
        val oled = appColorScheme(true, true)
        assertEquals(Color.Black, oled.background)
        assertEquals(Color.Black, oled.surface)
        assertNotEquals(Color.Black, oled.surfaceContainerLow)
        assertEquals(dark.onSurfaceVariant, oled.onSurfaceVariant)
        assertEquals(dark.primary, oled.primary)
        assertEquals(appColorScheme(false), appColorScheme(false, true))
    }

    @Test fun inputOutlinesRemainVisibleAgainstTheComposer() {
        schemes.forEach { scheme ->
            assertTrue(contrast(scheme.outline, scheme.surfaceContainerLow) >= 3f)
            assertTrue(contrast(scheme.primary, scheme.surfaceContainerLow) >= 3f)
        }
    }

    @Test fun personaLabelsAdaptEvenForBlackWhiteAndSaturatedCustomColors() {
        val accents = listOf(Color.Black, Color.White, Color.Red, Color.Blue, Color.Green,
            Color.Yellow, Color(0xFF7C3AED), Color(0xFFDB2777), Color(0xFFCA8A04))
        for (scheme in schemes) for (accent in accents) {
            val background = accent.copy(alpha = 0.12f).compositeOver(scheme.surfaceContainerLow)
            assertTrue(contrast(readablePersonaColor(accent, background), background) >= 4.5f)
            assertTrue(contrast(personaInitialColor(accent), accent) >= 4.5f)
        }
    }
}
