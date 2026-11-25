package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
        val modes = listOf("auto", "enabled", "disabled")
        ClawSeedSegmentedPill(
            options = listOf(stringResource(R.string.settings_vision_auto), stringResource(R.string.settings_vision_on), stringResource(R.string.settings_vision_off)),
            selectedIndex = modes.indexOf(value),
            onSelect = { onChange(modes[it]) },
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            stringResource(R.string.settings_vision_hint),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
