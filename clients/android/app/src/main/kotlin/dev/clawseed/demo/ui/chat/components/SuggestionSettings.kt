package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.components.*
import org.json.JSONArray
import org.json.JSONObject

internal data class SuggestionCard(val title: String, val prompt: String, val icon: Int, val visible: Boolean = true)

internal val suggestionIcons: List<ImageVector> = listOf(Icons.Default.Build, Icons.Default.Favorite, Icons.Default.DateRange, Icons.Default.Create)

internal fun decodeSuggestions(json: String, persona: String?, defaults: List<SuggestionCard>): List<SuggestionCard> = runCatching {
    val key = if (persona == null) "default" else "persona:$persona"
    val array = JSONObject(json).optJSONArray(key) ?: return defaults
    if (array.length() != 4) return defaults
    (0 until array.length()).map { index ->
        val card = array.getJSONObject(index)
        SuggestionCard(card.getString("title"), card.getString("prompt"), card.optInt("icon", 0).coerceIn(0, 3), card.optBoolean("visible", true))
    }
}.getOrDefault(defaults)

internal fun encodeSuggestions(cards: List<SuggestionCard>): String = JSONArray().apply {
    cards.forEach { card -> put(JSONObject().put("title", card.title).put("prompt", card.prompt).put("icon", card.icon).put("visible", card.visible)) }
}.toString()

@Composable
internal fun SuggestionSettingsDialog(
    initial: List<SuggestionCard>,
    onDismiss: () -> Unit,
    onSave: (List<SuggestionCard>) -> Unit,
    onReset: () -> Unit,
) {
    var cards by remember { mutableStateOf(initial) }
    fun update(index: Int, card: SuggestionCard) { cards = cards.toMutableList().also { it[index] = card } }
    ClawSeedFormDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.suggestion_edit_title)) },
        text = {
            Text(stringResource(R.string.suggestion_edit_hint), style = MaterialTheme.typography.bodySmall)
            cards.forEachIndexed { index, card ->
                SettingsGroup(Modifier.fillMaxWidth().padding(top = 12.dp)) {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Text(stringResource(R.string.suggestion_card_number, index + 1), style = MaterialTheme.typography.titleSmall)
                            Row {
                                ClawSeedIconButton(onClick = { cards = cards.toMutableList().also { java.util.Collections.swap(it, index, index - 1) } }, enabled = index > 0) {
                                    Icon(Icons.Default.KeyboardArrowUp, contentDescription = stringResource(R.string.suggestion_move_up))
                                }
                                ClawSeedIconButton(onClick = { cards = cards.toMutableList().also { java.util.Collections.swap(it, index, index + 1) } }, enabled = index < cards.lastIndex) {
                                    Icon(Icons.Default.KeyboardArrowDown, contentDescription = stringResource(R.string.suggestion_move_down))
                                }
                                ClawSeedSwitch(checked = card.visible, onCheckedChange = { update(index, card.copy(visible = it)) })
                            }
                        }
                        ClawSeedTextField(value = card.title, onValueChange = { update(index, card.copy(title = it)) }, label = { Text(stringResource(R.string.suggestion_title_label)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        ClawSeedTextField(value = card.prompt, onValueChange = { update(index, card.copy(prompt = it)) }, label = { Text(stringResource(R.string.suggestion_prompt_label)) }, minLines = 2, modifier = Modifier.fillMaxWidth())
                        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                            suggestionIcons.forEachIndexed { iconIndex, icon ->
                                ClawSeedChoiceChip(selected = card.icon == iconIndex, onClick = { update(index, card.copy(icon = iconIndex)) }, label = {
                                    Icon(icon, contentDescription = stringResource(R.string.suggestion_icon_number, iconIndex + 1), modifier = Modifier.size(18.dp))
                                })
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { ClawSeedButton(onClick = { onSave(cards) }, enabled = cards.all { !it.visible || (it.title.isNotBlank() && it.prompt.isNotBlank()) }) { Text(stringResource(R.string.suggestion_save)) } },
        dismissButton = {
            ClawSeedTextButton(onClick = onReset) { Text(stringResource(R.string.suggestion_restore_defaults)) }
            ClawSeedTextButton(onClick = onDismiss) { Text(stringResource(R.string.common_cancel)) }
        },
    )
}
