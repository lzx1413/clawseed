package dev.clawseed.demo.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.ClawSeedShapes

enum class GlassLevel { Chrome, Sheet, Surface, Pill, Elevated }

internal fun glassFill(colors: androidx.compose.material3.ColorScheme, level: GlassLevel) = when (level) {
    GlassLevel.Surface -> colors.surface
    GlassLevel.Chrome -> colors.surfaceContainerLowest
    GlassLevel.Elevated -> colors.surface.copy(alpha = 0.88f)
    GlassLevel.Sheet -> colors.surfaceContainerLow
    GlassLevel.Pill -> colors.surface
}

/**
 * Local glass surface with a solid fallback. Blur is intentionally not applied
 * here so Android 11 and reduced-motion devices keep the same readable layout.
 * A future local blur modifier can be inserted before the fill without changing
 * callers.
 */
@Composable
fun GlassContainer(
    modifier: Modifier = Modifier,
    level: GlassLevel = GlassLevel.Surface,
    shape: Shape = ClawSeedShapes.Medium,
    interactive: Boolean = false,
    shadowElevation: Dp = 0.dp,
    content: @Composable () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val fill = glassFill(colors, level)
    val border = when {
        interactive -> colors.primary
        level == GlassLevel.Chrome -> colors.outlineVariant
        level == GlassLevel.Elevated -> colors.outline.copy(alpha = 0.46f)
        else -> colors.outlineVariant.copy(alpha = 0.56f)
    }

    Surface(
        modifier = modifier,
        shape = shape,
        color = fill,
        contentColor = colors.onSurface,
        tonalElevation = 0.dp,
        shadowElevation = shadowElevation,
        border = androidx.compose.foundation.BorderStroke(1.dp, border),
    ) {
        content()
    }
}

/** Compatibility column wrapper; compact surfaces use GlassContainer directly. */
@Composable
fun GlassSurface(
    modifier: Modifier = Modifier, level: GlassLevel = GlassLevel.Surface,
    shape: Shape = ClawSeedShapes.Medium, interactive: Boolean = false,
    shadowElevation: Dp = 0.dp, content: @Composable ColumnScope.() -> Unit,
) {
    GlassContainer(modifier, level, shape, interactive, shadowElevation) {
        Column(Modifier.fillMaxWidth(), content = content)
    }
}
