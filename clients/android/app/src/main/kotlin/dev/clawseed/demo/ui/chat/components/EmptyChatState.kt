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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import dev.clawseed.demo.data.LocalStore
import dev.clawseed.demo.ui.components.ClawSeedTextButton
import kotlinx.coroutines.launch
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import java.time.LocalTime
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
    persona: String? = null,
) {
    val lifecycleOwner = LocalLifecycleOwner.current
    val currentHour by produceState(LocalTime.now().hour, lifecycleOwner) {
        lifecycleOwner.lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            while (isActive) {
                value = LocalTime.now().hour
                delay(60_000L)
            }
        }
    }
    val greeting = when (currentHour) {
        in 5..11 -> R.string.chat_empty_greeting_morning
        in 12..17 -> R.string.chat_empty_greeting_afternoon
        else -> R.string.chat_empty_greeting_evening
    }
    val defaults = listOf(
        stringResource(R.string.chat_empty_suggestion_tools),
        stringResource(R.string.chat_empty_suggestion_memory),
        stringResource(R.string.chat_empty_suggestion_schedule),
        stringResource(R.string.chat_empty_suggestion_idea),
    ).mapIndexed { index, label -> SuggestionCard(label, label, index) }
    val context = LocalContext.current
    val store = remember(context) { LocalStore(context.applicationContext) }
    val saved by store.suggestionCards.collectAsState(initial = "{}")
    val cards = remember(saved, persona, defaults) { decodeSuggestions(saved, persona, defaults) }
    val scope = rememberCoroutineScope()
    var editing by remember(persona) { mutableStateOf(false) }
    if (editing) {
        SuggestionSettingsDialog(initial = cards, onDismiss = { editing = false },
            onSave = { updated -> scope.launch { store.setSuggestionCards(persona, encodeSuggestions(updated)) }; editing = false },
            onReset = { scope.launch { store.setSuggestionCards(persona, null) }; editing = false })
    }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp, vertical = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(
            text = stringResource(greeting),
            style = MaterialTheme.typography.headlineMedium,
            color = MaterialTheme.colorScheme.onBackground,
        )
        Text(
            text = stringResource(R.string.chat_empty_prompt),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        ClawSeedTextButton(onClick = { editing = true }) { Text(stringResource(R.string.suggestion_edit_title)) }
        BoxWithConstraints {
        val columns = if (maxWidth < 320.dp || LocalDensity.current.fontScale >= 1.3f) 1 else 2
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            cards.filter { it.visible }.chunked(columns).forEachIndexed { rowIndex, row ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    row.forEachIndexed { columnIndex, card ->
                        val index = rowIndex * columns + columnIndex
                        EmptySuggestion(
                            label = card.title,
                            icon = suggestionIcons[card.icon],
                            onClick = { onSuggestion(card.prompt) },
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
