package dev.clawseed.demo.ui.components

import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

@Composable
fun ClawSeedSwitch(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = MaterialTheme.colorScheme
    Switch(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = modifier,
        enabled = enabled,
        colors = SwitchDefaults.colors(
            checkedThumbColor = colors.onPrimary,
            checkedTrackColor = colors.primary,
            checkedBorderColor = colors.primary,
            uncheckedThumbColor = colors.outline,
            uncheckedTrackColor = colors.surfaceContainerHighest,
            uncheckedBorderColor = colors.outline,
        ),
    )
}

@Composable
fun ClawSeedCheckbox(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = MaterialTheme.colorScheme
    Checkbox(
        checked = checked,
        onCheckedChange = onCheckedChange,
        modifier = modifier,
        enabled = enabled,
        colors = CheckboxDefaults.colors(
            checkedColor = colors.primary,
            uncheckedColor = colors.onSurfaceVariant,
            checkmarkColor = colors.onPrimary,
            disabledCheckedColor = colors.onSurface.copy(alpha = 0.38f),
            disabledUncheckedColor = colors.onSurface.copy(alpha = 0.12f),
        ),
    )
}

@Composable
fun ClawSeedTabRow(
    selectedTabIndex: Int,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    TabRow(
        selectedTabIndex = selectedTabIndex,
        modifier = modifier,
        containerColor = colors.surface,
        contentColor = colors.onSurface,
        divider = { HorizontalDivider(color = colors.outlineVariant) },
        tabs = content,
    )
}

@Composable
fun ClawSeedTab(
    selected: Boolean,
    onClick: () -> Unit,
    text: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = MaterialTheme.colorScheme
    Tab(
        selected = selected,
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        selectedContentColor = colors.primary,
        unselectedContentColor = colors.onSurfaceVariant,
        text = text,
    )
}
