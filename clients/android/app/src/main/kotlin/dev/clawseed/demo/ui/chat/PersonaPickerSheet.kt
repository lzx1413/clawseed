package dev.clawseed.demo.ui.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.persona.PersonaDot
import dev.clawseed.demo.ui.persona.personaContainerColor
import dev.clawseed.demo.ui.persona.personaSummary
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.PersonaInfo
import kotlinx.coroutines.launch

/**
 * A persona entry selectable in the picker. [name] is `null` for the always
 * present "Default" (global agent) option.
 */
private data class PickerEntry(val name: String?, val subtitle: String?)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PersonaPickerSheet(
    onDismiss: () -> Unit,
    onStart: (persona: String?) -> Unit,
    onManage: () -> Unit = {},
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var loading by remember { mutableStateOf(true) }
    var personas by remember { mutableStateOf<List<PersonaInfo>>(emptyList()) }
    var loadError by remember { mutableStateOf(false) }
    // Selected persona: null = Default. Saved across config changes.
    var selected by rememberSaveable { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        if (!ClawSeedAndroid.isInitialized) {
            loading = false
            loadError = true
            return@LaunchedEffect
        }
        ClawSeedAndroid.gatewayClient().personas()
            .onSuccess { personas = it.filter { info -> info.isPersona }; loading = false }
            .onFailure { loadError = true; loading = false }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(modifier = Modifier.padding(horizontal = 24.dp).padding(bottom = 24.dp)) {
            Text(
                text = stringResource(R.string.persona_new_chat_title),
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(vertical = 8.dp),
            )

            val entries = buildList {
                add(PickerEntry(null, null)) // Default
                personas.forEach { add(PickerEntry(it.name, it.subtitle())) }
            }

            when {
                loading -> Box(
                    modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp).padding(24.dp),
                    contentAlignment = Alignment.Center,
                ) { CircularProgressIndicator() }

                loadError -> Text(
                    text = stringResource(R.string.persona_load_failed),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(vertical = 16.dp),
                )

                else -> LazyColumn(
                    modifier = Modifier.heightIn(max = 360.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    items(entries) { entry ->
                        val isSelected = selected == entry.name
                        val persona = personas.find { it.name == entry.name }
                        val rowBackground = persona
                            ?.let { personaContainerColor(it.name, it.color) }
                            ?: if (isSelected) MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.7f) else Color.Transparent
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .background(rowBackground)
                                .clickable { selected = entry.name }
                                .padding(vertical = 10.dp, horizontal = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(selected = isSelected, onClick = { selected = entry.name })
                            Spacer(Modifier.width(12.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(
                                    text = entry.name ?: stringResource(R.string.persona_default),
                                    style = MaterialTheme.typography.bodyLarge,
                                )
                                val sub = entry.subtitle ?: stringResource(R.string.persona_default_desc)
                                if (sub.isNotEmpty()) {
                                    Text(
                                        text = sub,
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                }
                            }
                            if (persona != null) {
                                PersonaDot(
                                    persona.name,
                                    Modifier.size(28.dp),
                                    showInitial = true,
                                    avatar = persona.avatar,
                                    color = persona.color,
                                )
                            }
                        }
                    }
                    if (personas.isEmpty()) {
                        item {
                            Text(
                                text = stringResource(R.string.persona_picker_empty),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.padding(12.dp),
                            )
                        }
                    }
                }
            }

            Spacer(Modifier.padding(4.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                OutlinedButton(onClick = onDismiss) {
                    Text(stringResource(R.string.common_cancel))
                }
                Spacer(Modifier.width(8.dp))
                OutlinedButton(onClick = onManage) {
                    Text(stringResource(R.string.persona_manage))
                }
                Spacer(Modifier.width(8.dp))
                Button(
                    onClick = { onStart(selected) },
                    enabled = !loading && !loadError,
                ) {
                    Text(stringResource(R.string.persona_start))
                }
            }
        }
    }
}

/** One-line summary of a persona's overrides, for the picker subtitle. */
private fun PersonaInfo.subtitle(): String {
    return personaSummary(this)
}
