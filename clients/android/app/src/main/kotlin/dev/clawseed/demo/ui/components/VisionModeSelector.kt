package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R

@Composable
fun VisionModeSelector(
    value: String,
    onChange: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(stringResource(R.string.settings_vision))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            val options = buildList<Pair<String, Int>> {
                add("auto" to R.string.settings_vision_auto)
                add("enabled" to R.string.settings_vision_on)
                add("disabled" to R.string.settings_vision_off)
            }
            options.forEach { (mode, label) ->
                FilterChip(
                    selected = value == mode,
                    onClick = { onChange(mode) },
                    label = { Text(stringResource(label)) },
                )
            }
        }
        Text(
            stringResource(R.string.settings_vision_hint),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
