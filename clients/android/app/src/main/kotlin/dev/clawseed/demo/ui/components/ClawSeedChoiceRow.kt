package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.RadioButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import dev.clawseed.demo.ui.theme.ClawSeedSpacing

/** A single accessible selection target with the shared card and settings-row layout. */
@Composable
fun ClawSeedChoiceRow(
    title: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    multiple: Boolean = false,
    enabled: Boolean = true,
) {
    val selection = if (multiple) {
        Modifier.toggleable(selected, enabled = enabled, role = Role.Checkbox, onValueChange = { onClick() })
    } else {
        Modifier.selectable(selected, enabled = enabled, role = Role.RadioButton, onClick = onClick)
    }
    ClawSeedCard(modifier = modifier, selected = selected, muted = !enabled) {
        SettingsRow(
            title = title,
            subtitle = subtitle,
            enabled = enabled,
            modifier = selection.padding(horizontal = ClawSeedSpacing.Medium),
            trailing = {
                if (multiple) {
                    ClawSeedCheckbox(selected, onCheckedChange = null, enabled = enabled,
                        modifier = Modifier.clearAndSetSemantics {})
                } else {
                    RadioButton(selected, onClick = null, enabled = enabled,
                        modifier = Modifier.clearAndSetSemantics {})
                }
            },
        )
    }
}
