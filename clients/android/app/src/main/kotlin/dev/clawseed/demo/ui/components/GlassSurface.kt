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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlurEffect
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.layer.GraphicsLayer
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalGraphicsContext
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import android.os.Build
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedSizes
import dev.clawseed.demo.ui.theme.ClawSeedSpacing

enum class GlassLevel { Chrome, Sheet, Surface, Pill, Elevated }

private class GlassBackdropState(val layer: GraphicsLayer) {
    var hostCoordinates by mutableStateOf<LayoutCoordinates?>(null)
}

private val LocalGlassBackdrop = staticCompositionLocalOf<GlassBackdropState?> { null }

/** Records a background subtree for the glass controls drawn above it. */
@Composable
fun GlassBackdropHost(
    modifier: Modifier = Modifier,
    background: @Composable BoxScope.() -> Unit,
    content: @Composable BoxScope.() -> Unit,
) {
    val graphicsContext = LocalGraphicsContext.current
    val state = remember(graphicsContext) {
        GlassBackdropState(graphicsContext.createGraphicsLayer())
    }
    val density = androidx.compose.ui.platform.LocalDensity.current
    val layoutDirection = LocalLayoutDirection.current

    DisposableEffect(state, graphicsContext) {
        onDispose { graphicsContext.releaseGraphicsLayer(state.layer) }
    }

    androidx.compose.runtime.CompositionLocalProvider(LocalGlassBackdrop provides state) {
        Box(
            modifier = modifier.onGloballyPositioned { state.hostCoordinates = it },
        ) {
            androidx.compose.runtime.CompositionLocalProvider(LocalGlassBackdrop provides null) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .drawWithCache {
                            onDrawWithContent {
                                val contentScope = this
                                state.layer.record(
                                    density,
                                    layoutDirection,
                                    IntSize(size.width.roundToInt(), size.height.roundToInt()),
                                ) { contentScope.drawContent() }
                                contentScope.drawContent()
                            }
                        },
                    content = background,
                )
            }
            androidx.compose.runtime.CompositionLocalProvider(LocalGlassBackdrop provides state) {
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
    GlassLevel.Pill -> colors.surface.copy(alpha = 0.64f)
}

/**
 * Local glass surface with a translucent fallback. Inside a GlassBackdropHost
 * it also draws a clipped, sampled copy of the background with RenderEffect.
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
    val backdrop = LocalGlassBackdrop.current
    var backdropOffset by remember(backdrop) { mutableStateOf(Offset.Zero) }
    val fill = glassFill(colors, level)
    val border = when {
        interactive -> colors.primary
        level == GlassLevel.Chrome -> colors.outlineVariant
        level == GlassLevel.Elevated -> colors.outline.copy(alpha = 0.46f)
        else -> colors.outlineVariant.copy(alpha = 0.56f)
    }

    val backdropModifier = if (backdrop == null) {
        Modifier
    } else {
        Modifier
            .onGloballyPositioned { coordinates ->
                val host = backdrop.hostCoordinates
                if (host != null && host.isAttached && coordinates.isAttached) {
                    backdropOffset = host.localPositionOf(coordinates, Offset.Zero)
                }
            }
            .drawWithCache {
                val clip = shape.createOutline(size, layoutDirection, this).toPath()
                val blurRadius = 18.dp.toPx()
                val blurSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
                onDrawWithContent {
                    if (blurSupported) {
                        val layer = backdrop.layer
                        val previousEffect = layer.renderEffect
                        try {
                            layer.renderEffect = BlurEffect(blurRadius, blurRadius)
                            clipPath(clip) {
                                translate(-backdropOffset.x, -backdropOffset.y) {
                                    drawLayer(layer)
                                }
                            }
                        } finally {
                            layer.renderEffect = previousEffect
                        }
                    }
                    drawContent()
                }
            }
    }

    Surface(
        modifier = backdropModifier.then(modifier),
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

private fun Outline.toPath(): Path = when (this) {
    is Outline.Rectangle -> Path().apply { addRect(rect) }
    is Outline.Rounded -> Path().apply { addRoundRect(roundRect) }
    is Outline.Generic -> path
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
