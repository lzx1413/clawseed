package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Icon
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
        level = GlassLevel.Pill,
        shape = ClawSeedShapes.Pill,
        content = content,
    )
}

@Composable
fun ClawSeedFloatingIconButton(
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    dangerous: Boolean = false,
    tooltip: String? = null,
    content: @Composable () -> Unit,
) {
    ClawSeedFloatingSurface(modifier.size(ClawSeedSizes.TouchTarget)) {
        ClawSeedIconButton(
            onClick = onClick,
            modifier = Modifier.fillMaxSize(),
            enabled = enabled,
            dangerous = dangerous,
            contentDescription = contentDescription,
            tooltip = tooltip,
            content = content,
        )
    }
}

/** Shared add action used by toolbar and scaffold floating-action slots. */
@Composable
fun ClawSeedAddAction(
    onClick: () -> Unit,
    contentDescription: String,
    modifier: Modifier = Modifier,
) {
    ClawSeedFloatingIconButton(
        onClick = onClick,
        contentDescription = contentDescription,
        modifier = modifier,
    ) {
        Icon(Icons.Default.Add, contentDescription = null)
    }
}
