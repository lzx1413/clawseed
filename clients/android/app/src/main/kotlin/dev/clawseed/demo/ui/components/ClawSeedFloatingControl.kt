package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import dev.clawseed.demo.ui.theme.ClawSeedShadows
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedSizes

/** Shared height, surface and shadow for independently floating toolbar controls. */
@Composable
fun ClawSeedFloatingSurface(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    GlassContainer(
        modifier = modifier.height(ClawSeedSizes.TouchTarget)
            .dropShadow(ClawSeedShapes.Pill, ClawSeedShadows.FloatingControl),
        level = GlassLevel.Chrome,
        shape = ClawSeedShapes.Pill,
        content = content,
    )
}

@Composable
fun ClawSeedFloatingIconButton(
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    ClawSeedFloatingSurface(modifier.width(ClawSeedSizes.TouchTarget)) {
        ClawSeedIconButton(
            onClick = onClick,
            modifier = Modifier.fillMaxSize(),
            contentDescription = contentDescription,
            content = content,
        )
    }
}
