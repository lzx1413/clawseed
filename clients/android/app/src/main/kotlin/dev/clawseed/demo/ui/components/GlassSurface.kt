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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.luminance
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.chrisbanes.haze.HazeInput
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import dev.chrisbanes.haze.rememberHazeState
import dev.chrisbanes.haze.glass.GlassOptics
import dev.chrisbanes.haze.glass.OpticalSizeValue
import dev.chrisbanes.haze.glass.GlassStyle
import dev.chrisbanes.haze.glass.hazeGlass
import dev.chrisbanes.haze.glass.material3.material3
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedSizes
import dev.clawseed.demo.ui.theme.ClawSeedSpacing

enum class GlassLevel { Chrome, Sheet, Surface, Pill, FloatingInput, Elevated }

private val LocalHazeState = staticCompositionLocalOf<HazeState?> { null }

/** Captures the background subtree for Haze-backed glass controls. */
@Composable
fun GlassBackdropHost(
    modifier: Modifier = Modifier,
    background: @Composable BoxScope.() -> Unit,
    content: @Composable BoxScope.() -> Unit,
) {
    val hazeState = rememberHazeState()

    androidx.compose.runtime.CompositionLocalProvider(LocalHazeState provides hazeState) {
        Box(modifier = modifier) {
            androidx.compose.runtime.CompositionLocalProvider(LocalHazeState provides null) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .hazeSource(hazeState),
                    content = background,
                )
            }
            androidx.compose.runtime.CompositionLocalProvider(LocalHazeState provides hazeState) {
                content()
            }
        }
    }
}

internal fun glassFill(colors: androidx.compose.material3.ColorScheme, level: GlassLevel) = when (level) {
    GlassLevel.Surface -> colors.surface
    GlassLevel.Chrome -> colors.surfaceContainerLowest.copy(alpha = 0.68f)
    GlassLevel.Elevated -> colors.surface.copy(alpha = 0.88f)
    GlassLevel.Sheet -> colors.surfaceContainerLow
    GlassLevel.Pill -> colors.surface.copy(alpha = 0.82f)
    GlassLevel.FloatingInput -> colors.surface.copy(alpha = 0.76f)
}

/**
 * Glass surface with a translucent fallback. Inside a GlassBackdropHost it
 * uses Haze to sample and blur the registered background subtree.
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
    val hazeState = LocalHazeState.current
    val fill = glassFill(colors, level)
    val lightSurface = colors.surface.luminance() > 0.5f
    val border = when {
        interactive -> colors.primary
        hazeState != null -> if (lightSurface) Color.White.copy(alpha = 0.55f) else colors.outlineVariant.copy(alpha = 0.3f)
        level == GlassLevel.Chrome -> colors.outlineVariant
        level == GlassLevel.FloatingInput -> colors.outlineVariant.copy(alpha = 0.32f)
        level == GlassLevel.Elevated -> colors.outline.copy(alpha = 0.46f)
        else -> colors.outlineVariant.copy(alpha = 0.56f)
    }

    val inputGlass = level == GlassLevel.FloatingInput
    // Diffuse text behind the editor more strongly than behind compact controls.
    val glassStyle = GlassStyle.regular
        .material3(tint = colors.surface.copy(alpha = if (inputGlass) 0.08f else 0.035f))
        .then {
            (shape as? RoundedCornerShape)?.let { shape(it) }
            optics(GlassOptics.regular.copy(
                depth = OpticalSizeValue.Fixed(1f),
                blurRadius = OpticalSizeValue.Fixed(if (inputGlass) 28.dp else 16.dp),
                refractionDisplacement = if (inputGlass) 8.dp else 14.dp,
            ))
            edgeShadow(Color.Black.copy(alpha = if (lightSurface) 0.025f else 0.12f))
            specularIntensity(if (inputGlass) 0.26f else 0.34f)
            ambientResponse(0.12f)
            edgeSoftness(1.dp)
            contrast(0f)
            whitePoint(if (lightSurface) { if (inputGlass) 0.35f else 0.28f } else -0.18f)
            chromaMultiplier(1.15f)
        }

    val hazeModifier = if (hazeState == null) {
        Modifier
    } else {
        Modifier
            .clip(shape)
            .hazeGlass(
                input = HazeInput.Sources(hazeState),
                style = glassStyle,
            )
    }

    Surface(
        modifier = modifier.then(hazeModifier),
        shape = shape,
        color = if (hazeState == null) fill else Color.Transparent,
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
 * GlassButton keeps its readable translucent fill when it is used outside a
 * GlassBackdropHost; inside a host, the shared backdrop layer supplies the
 * sampled blur without changing button call sites.
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
