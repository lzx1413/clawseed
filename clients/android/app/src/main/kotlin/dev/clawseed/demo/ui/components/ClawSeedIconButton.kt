package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.ui.graphics.graphicsLayer
import dev.clawseed.demo.ui.theme.ClawSeedMotion
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.*
import dev.clawseed.demo.ui.theme.ClawSeedSizes

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ClawSeedIconButton(onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    contentDescription: String? = null, filled: Boolean = false, dangerous: Boolean = false,
    tooltip: String? = null, content: @Composable () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val pressed by interactions.collectIsPressedAsState()
    val scale by animateFloatAsState(if (pressed && enabled) 0.94f else 1f,
        animationSpec = tween(ClawSeedMotion.Press), label = "iconPress")
    val visual: @Composable () -> Unit = {
        Box(Modifier.graphicsLayer { scaleX = scale; scaleY = scale }) { content() }
    }
    val target = modifier.sizeIn(minWidth = ClawSeedSizes.TouchTarget, minHeight = ClawSeedSizes.TouchTarget)
        .semantics { contentDescription?.let { this.contentDescription = it } }
    val button: @Composable () -> Unit = {
        if (filled) FilledIconButton(onClick, target, enabled,
            colors = IconButtonDefaults.filledIconButtonColors(
                containerColor = if (dangerous) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                contentColor = if (dangerous) MaterialTheme.colorScheme.onError else MaterialTheme.colorScheme.onPrimary), interactionSource = interactions, content = visual)
        else IconButton(onClick, target, enabled, colors = IconButtonDefaults.iconButtonColors(
            contentColor = if (dangerous) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant), interactionSource = interactions, content = visual)
    }
    if (tooltip == null) button() else TooltipBox(
        positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
        tooltip = { PlainTooltip { Text(tooltip) } }, state = rememberTooltipState(), content = button)
}
