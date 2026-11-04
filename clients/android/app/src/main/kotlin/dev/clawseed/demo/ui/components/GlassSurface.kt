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

enum class GlassLevel { Surface, Elevated }

/**
 * Local glass surface with a solid fallback. Blur is intentionally not applied
 * here so Android 11 and reduced-motion devices keep the same readable layout.
 * A future local blur modifier can be inserted before the fill without changing
 * callers.
 */
@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    level: GlassLevel = GlassLevel.Surface,
    shape: Shape = ClawSeedShapes.Medium,
    interactive: Boolean = false,
    shadowElevation: Dp = 0.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val fill = when (level) {
        GlassLevel.Surface -> colors.surfaceContainerLow.copy(alpha = 0.94f)
        GlassLevel.Elevated -> colors.surface.copy(alpha = 0.88f)
    }
    val border = when {
        interactive -> colors.primary.copy(alpha = 0.52f)
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
        Column(modifier = Modifier.fillMaxWidth(), content = content)
    }
}
