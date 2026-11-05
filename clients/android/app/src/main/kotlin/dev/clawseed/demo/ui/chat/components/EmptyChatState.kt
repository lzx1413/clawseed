package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material.icons.filled.Create
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R

@Composable
fun EmptyChatState(
    onSuggestion: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val suggestions = listOf(
        R.string.chat_empty_suggestion_tools,
        R.string.chat_empty_suggestion_memory,
        R.string.chat_empty_suggestion_schedule,
        R.string.chat_empty_suggestion_idea,
    )
    val icons = listOf(Icons.Default.Build, Icons.Default.Favorite, Icons.Default.DateRange, Icons.Default.Create)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp, vertical = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            text = stringResource(R.string.chat_empty_greeting),
            style = MaterialTheme.typography.headlineMedium,
            color = MaterialTheme.colorScheme.onBackground,
        )
        Text(
            text = stringResource(R.string.chat_empty_prompt),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        BoxWithConstraints {
        val columns = if (maxWidth < 320.dp || LocalDensity.current.fontScale >= 1.3f) 1 else 2
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            suggestions.chunked(columns).forEachIndexed { rowIndex, row ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    row.forEachIndexed { columnIndex, labelRes ->
                        val index = rowIndex * columns + columnIndex
                        val label = stringResource(labelRes)
                        EmptySuggestion(
                            label = label,
                            icon = icons[index],
                            onClick = { onSuggestion(label) },
                            modifier = Modifier
                                .weight(1f)
                                .testTag("chat_empty_suggestion_$index"),
                        )
                    }
                }
            }
        }
        }
    }
}

@Composable
private fun EmptySuggestion(
    label: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    dev.clawseed.demo.ui.components.GlassContainer(
        modifier = modifier,
        level = dev.clawseed.demo.ui.components.GlassLevel.Pill,
        shape = dev.clawseed.demo.ui.theme.ClawSeedShapes.Card,
    ) {
        Column(Modifier.clickable(role = Role.Button, onClick = onClick).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Surface(shape = dev.clawseed.demo.ui.theme.ClawSeedShapes.Small,
                color = MaterialTheme.colorScheme.primaryContainer,
                contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
                Icon(icon, null, Modifier.padding(8.dp).size(20.dp))
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, Modifier.size(18.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}
