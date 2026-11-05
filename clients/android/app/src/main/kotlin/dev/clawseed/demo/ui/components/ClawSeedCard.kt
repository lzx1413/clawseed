package dev.clawseed.demo.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.ClawSeedShapes

/** Shared neutral card; selection and muted state remain independent of click behavior. */
@Composable
fun ClawSeedCard(
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    muted: Boolean = false,
    containerColor: Color = Color.Unspecified,
    onClick: (() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val fill = when {
        selected -> colors.primaryContainer
        containerColor != Color.Unspecified -> containerColor
        muted -> colors.surfaceContainerLow
        else -> colors.surface
    }
    val border = BorderStroke(1.dp, if (selected) colors.primary else colors.outlineVariant)
    val body: @Composable () -> Unit = { Column(Modifier.fillMaxWidth(), content = content) }
    if (onClick == null) {
        Surface(modifier, shape = ClawSeedShapes.Card, color = fill,
            contentColor = colors.onSurface, border = border, content = body)
    } else {
        Surface(onClick = onClick, modifier = modifier, shape = ClawSeedShapes.Card, color = fill,
            contentColor = colors.onSurface, border = border, content = body)
    }
}
