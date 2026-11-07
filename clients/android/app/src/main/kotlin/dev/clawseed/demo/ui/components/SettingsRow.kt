package dev.clawseed.demo.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.*

@Composable
fun SettingsRow(title: String, modifier: Modifier = Modifier, subtitle: String? = null,
    value: String? = null, icon: (@Composable () -> Unit)? = null, onClick: (() -> Unit)? = null,
    enabled: Boolean = true, trailing: (@Composable () -> Unit)? = null) {
    Row(modifier.fillMaxWidth().heightIn(min = if (subtitle == null) ClawSeedSizes.Row else ClawSeedSizes.TwoLineRow)
        .then(if (onClick != null) Modifier.clickable(enabled = enabled, onClick = onClick) else Modifier)
        .padding(vertical = ClawSeedSpacing.Small), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Medium)) {
        if (icon != null) Box(Modifier.size(ClawSeedSizes.IconSlot), contentAlignment = Alignment.Center) { icon() }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.rowTitle,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (enabled) 1f else 0.38f))
            subtitle?.let { Text(it, style = MaterialTheme.typography.supporting, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            value?.let { Text(it, style = MaterialTheme.typography.rowTitle, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        if (trailing != null) trailing() else if (onClick != null) Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null)
    }
}

@Composable
fun SettingsSwitchRow(title: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier, subtitle: String? = null, enabled: Boolean = true,
    icon: (@Composable () -> Unit)? = null) {
    SettingsRow(title, modifier.toggleable(checked, enabled = enabled, role = Role.Switch,
        onValueChange = onCheckedChange), subtitle, icon = icon, enabled = enabled,
        trailing = { ClawSeedSwitch(checked, onCheckedChange = null, enabled = enabled,
            modifier = Modifier.clearAndSetSemantics {}) })
}

@Composable
fun SettingsCheckboxRow(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    enabled: Boolean = true,
) {
    SettingsRow(
        title = title,
        subtitle = subtitle,
        enabled = enabled,
        modifier = modifier.toggleable(
            value = checked, enabled = enabled, role = Role.Checkbox,
            onValueChange = onCheckedChange,
        ),
        trailing = {
            ClawSeedCheckbox(
                checked = checked, onCheckedChange = null, enabled = enabled,
                modifier = Modifier.clearAndSetSemantics {},
            )
        },
    )
}
