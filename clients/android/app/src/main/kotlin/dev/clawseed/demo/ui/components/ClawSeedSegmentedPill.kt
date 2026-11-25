package dev.clawseed.demo.ui.components

import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedSizes
import dev.clawseed.demo.ui.theme.ClawSeedSpacing

/** Equal-width segments; large text and narrow containers use a vertical group. */
@Composable
fun ClawSeedSegmentedPill(
    options: List<String>, selectedIndex: Int, onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier, enabled: Boolean = true,
) {
    BoxWithConstraints(modifier.selectableGroup()) {
        val density = LocalDensity.current
        val measurer = rememberTextMeasurer()
        val textStyle = MaterialTheme.typography.labelLarge
        val widest = options.maxOfOrNull { measurer.measure(it, textStyle, maxLines = 1).size.width } ?: 0
        val segmentWidth = maxOf(ClawSeedSizes.TouchTarget, with(density) { widest.toDp() } + ClawSeedSpacing.Medium * 2)
        val vertical = maxWidth < segmentWidth * options.size + ClawSeedSpacing.Tiny * (options.size + 1) || density.fontScale > 1.3f
        val select by rememberUpdatedState(onSelect)
        val current by rememberUpdatedState(selectedIndex)
        val direction = LocalLayoutDirection.current
        var preview by remember(options, enabled, vertical) { mutableStateOf<Int?>(null) }
        val drag = if (vertical || !enabled || options.isEmpty()) Modifier else Modifier.pointerInput(options.size, direction) {
            detectHorizontalDragGestures(
                onDragCancel = { preview = null },
                onDragEnd = {
                    val target = preview
                    preview = null
                    if (target != null && target != current) select(target)
                },
            ) { change, _ ->
                val index = (change.position.x * options.size / size.width).toInt().coerceIn(options.indices)
                preview = if (direction == LayoutDirection.Rtl) options.lastIndex - index else index
            }
        }
        Surface(shape = if (vertical) ClawSeedShapes.Card else ClawSeedShapes.Pill,
            color = MaterialTheme.colorScheme.surfaceContainer) {
            if (vertical) Column(Modifier.padding(ClawSeedSpacing.Tiny), verticalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Tiny)) {
                options.forEachIndexed { i, label ->
                    Segment(label, i == selectedIndex, enabled, { if (i != selectedIndex) onSelect(i) }, Modifier.fillMaxWidth())
                }
            } else Row(Modifier.then(drag).padding(ClawSeedSpacing.Tiny), horizontalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Tiny)) {
                options.forEachIndexed { i, label ->
                    Segment(label, i == (preview ?: selectedIndex), enabled, { if (i != selectedIndex) onSelect(i) }, Modifier.weight(1f))
                }
            }
        }
    }
}

@Composable
private fun Segment(label: String, selected: Boolean, enabled: Boolean, onClick: () -> Unit, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    Surface(modifier, shape = ClawSeedShapes.Pill,
        color = if (selected) colors.primary else colors.surfaceContainer,
        contentColor = (if (selected) colors.onPrimary else colors.onSurfaceVariant)
            .let { if (enabled) it else it.copy(alpha = 0.38f) }) {
        Box(Modifier.selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = onClick)
            .heightIn(min = ClawSeedSizes.TouchTarget).padding(horizontal = ClawSeedSpacing.Medium, vertical = ClawSeedSpacing.Small),
            contentAlignment = Alignment.Center) {
            Text(label, style = MaterialTheme.typography.labelLarge, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        }
    }
}
