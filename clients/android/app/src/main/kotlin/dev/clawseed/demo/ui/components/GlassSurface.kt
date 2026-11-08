package dev.clawseed.demo.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedSizes
import dev.clawseed.demo.ui.theme.ClawSeedSpacing

enum class GlassLevel { Chrome, Sheet, Surface, Pill, Elevated }

internal fun glassFill(colors: androidx.compose.material3.ColorScheme, level: GlassLevel) = when (level) {
    GlassLevel.Surface -> colors.surface
    GlassLevel.Chrome -> colors.surfaceContainerLowest.copy(alpha = 0.82f)
    GlassLevel.Elevated -> colors.surface.copy(alpha = 0.88f)
    GlassLevel.Sheet -> colors.surfaceContainerLow
    GlassLevel.Pill -> colors.surface.copy(alpha = 0.78f)
}

/**
 * Local glass surface with a translucent fallback. Compose's regular blur
 * modifier blurs a composable's own pixels rather than the content behind it,
 * so this layer uses translucency, a border, and elevation until a backdrop
 * filter can be supplied by the host surface.
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

/**
 * A compact frosted action surface. The translucent fill lets the surface
 * inherit color from whatever is behind it, while the border and pressed tint
 * keep the control readable over both light and dark content.
 *
 * Compose does not expose a per-composable backdrop filter, so this component
 * deliberately keeps a readable fill wherever backdrop sampling is unavailable.
 * A backdrop blur can be added inside this surface later without changing
 * button call sites.
 */
@Composable
fun GlassButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    dangerous: Boolean = false,
    contentPadding: PaddingValues = PaddingValues(
        horizontal = ClawSeedSpacing.ButtonHorizontal,
        vertical = ClawSeedSpacing.ButtonVertical,
    ),
    content: @Composable RowScope.() -> Unit,
) {
    val scheme = MaterialTheme.colorScheme
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val accent = if (dangerous) scheme.error else scheme.primary
    val containerColor = when {
        !enabled -> scheme.surfaceContainer.copy(alpha = 0.42f)
        pressed -> accent.copy(alpha = 0.26f)
        else -> scheme.surfaceContainerHigh.copy(alpha = 0.72f)
    }
    val contentColor = if (enabled) accent else scheme.onSurface.copy(alpha = 0.38f)
    val borderColor = when {
        !enabled -> scheme.outlineVariant.copy(alpha = 0.32f)
        pressed -> accent.copy(alpha = 0.78f)
        else -> scheme.outline.copy(alpha = 0.48f)
    }

    Surface(
        onClick = onClick,
        modifier = modifier
            .defaultMinSize(minHeight = ClawSeedSizes.Button),
        enabled = enabled,
        shape = ClawSeedShapes.Field,
        color = containerColor,
        contentColor = contentColor,
        tonalElevation = 0.dp,
        shadowElevation = if (enabled) 2.dp else 0.dp,
        border = BorderStroke(1.dp, borderColor),
        interactionSource = interactionSource,
    ) {
        Row(
            modifier = Modifier.padding(contentPadding),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Center,
            content = content,
        )
    }
}
