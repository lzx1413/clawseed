package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
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
        val vertical = maxWidth < (options.size * 88).dp || LocalDensity.current.fontScale > 1.3f
        Surface(shape = if (vertical) ClawSeedShapes.Card else ClawSeedShapes.Pill,
            color = MaterialTheme.colorScheme.surfaceContainer) {
            if (vertical) Column(Modifier.padding(ClawSeedSpacing.Tiny), verticalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Tiny)) {
                options.forEachIndexed { i, label ->
                    Segment(label, i == selectedIndex, enabled, { onSelect(i) }, Modifier.fillMaxWidth())
                }
            } else Row(Modifier.padding(ClawSeedSpacing.Tiny), horizontalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Tiny)) {
                options.forEachIndexed { i, label ->
                    Segment(label, i == selectedIndex, enabled, { onSelect(i) }, Modifier.weight(1f))
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
