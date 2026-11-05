package dev.clawseed.demo.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R

@Composable
fun ClawSeedExpandableSection(
    title: String, expanded: Boolean, onToggle: () -> Unit, subtitle: String? = null,
    content: @Composable () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        ClawSeedSectionHeader(title, expanded, onToggle, subtitle)
        AnimatedVisibility(visible = expanded, enter = expandVertically(), exit = shrinkVertically()) {
            Column(Modifier.padding(top = 12.dp, bottom = 8.dp)) { content() }
        }
    }
}

/** Shared heading for sections whose content can be composed inline or by a lazy parent. */
@Composable
fun ClawSeedSectionHeader(
    title: String, expanded: Boolean, onToggle: () -> Unit, subtitle: String? = null,
) {
    val stateLabel = stringResource(if (expanded) R.string.drawer_group_expanded else R.string.drawer_group_collapsed)
    androidx.compose.material3.Surface(
        shape = dev.clawseed.demo.ui.theme.ClawSeedShapes.Card,
        color = if (expanded) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        border = androidx.compose.foundation.BorderStroke(1.dp,
            if (expanded) MaterialTheme.colorScheme.primary.copy(alpha = 0.4f) else MaterialTheme.colorScheme.outlineVariant),
    ) {
        dev.clawseed.demo.ui.components.SettingsRow(
            title = title, subtitle = subtitle,
            modifier = Modifier.semantics { stateDescription = stateLabel }.padding(horizontal = 16.dp),
            onClick = onToggle,
            icon = {
                androidx.compose.material3.Surface(
                    shape = dev.clawseed.demo.ui.theme.ClawSeedShapes.Small,
                    color = if (expanded) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHigh,
                    contentColor = if (expanded) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurfaceVariant,
                ) {
                    Icon(if (expanded) Icons.Default.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                        null, Modifier.padding(4.dp).size(24.dp))
                }
            },
            trailing = {},
        )
    }
}
