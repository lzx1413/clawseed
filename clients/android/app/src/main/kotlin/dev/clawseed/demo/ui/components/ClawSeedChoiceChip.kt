package dev.clawseed.demo.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.*

enum class ChoiceType { Single, Multiple, Information }

@Composable
fun ClawSeedChoiceChip(selected: Boolean, onClick: () -> Unit, label: @Composable () -> Unit,
    modifier: Modifier = Modifier, enabled: Boolean = true, choiceType: ChoiceType = ChoiceType.Single,
    leadingIcon: (@Composable () -> Unit)? = null, trailingIcon: (@Composable () -> Unit)? = null,
) {
    val colors = MaterialTheme.colorScheme
    val selection = if (choiceType == ChoiceType.Information) Modifier else if (choiceType == ChoiceType.Single) Modifier.selectable(selected, enabled = enabled,
        role = Role.RadioButton, onClick = onClick) else Modifier.toggleable(selected, enabled = enabled,
        role = Role.Checkbox, onValueChange = { onClick() })
    Surface(modifier = modifier.heightIn(min = ClawSeedSizes.TouchTarget), shape = ClawSeedShapes.Pill,
        color = if (selected) colors.primaryContainer else colors.surfaceContainerLow,
        contentColor = (if (selected) colors.onPrimaryContainer else colors.onSurfaceVariant)
            .let { if (enabled) it else it.copy(alpha = 0.38f) },
        border = BorderStroke(1.dp, if (selected) colors.primary else colors.outlineVariant)) {
        Row(selection.padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (choiceType == ChoiceType.Multiple && selected) Icon(Icons.Default.Check, null, Modifier.size(18.dp))
            else leadingIcon?.invoke()
            ProvideTextStyle(MaterialTheme.typography.labelLarge) { label() }
            trailingIcon?.invoke()
        }
    }
}
