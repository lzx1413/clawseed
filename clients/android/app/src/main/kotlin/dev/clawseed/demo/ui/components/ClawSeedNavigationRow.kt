package dev.clawseed.demo.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedSizes
import dev.clawseed.demo.ui.theme.rowTitle

/** Drawer rows grow with wrapped labels and keep secondary actions separate. */
@Composable
fun ClawSeedNavigationRow(
    label: @Composable () -> Unit, selected: Boolean, onClick: () -> Unit,
    modifier: Modifier = Modifier, enabled: Boolean = true,
    icon: (@Composable () -> Unit)? = null, badge: (@Composable () -> Unit)? = null,
) {
    val colors = MaterialTheme.colorScheme
    Surface(modifier.padding(horizontal = 12.dp, vertical = 4.dp), shape = ClawSeedShapes.Field,
        color = if (selected) colors.primaryContainer else colors.surfaceContainerLow,
        border = if (selected) BorderStroke(1.dp, colors.primary) else null,
        contentColor = if (selected) colors.onPrimaryContainer else colors.onSurface) {
        Row(Modifier.selectable(selected, enabled = enabled, role = Role.Tab, onClick = onClick)
            .fillMaxWidth().heightIn(min = ClawSeedSizes.Row).padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            icon?.let { Box(Modifier.size(ClawSeedSizes.IconSlot), contentAlignment = Alignment.Center) { it() } }
            Box(Modifier.weight(1f)) { ProvideTextStyle(MaterialTheme.typography.rowTitle) { label() } }
            badge?.invoke()
        }
    }
}
