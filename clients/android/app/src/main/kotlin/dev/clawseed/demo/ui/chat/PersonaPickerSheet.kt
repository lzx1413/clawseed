package dev.clawseed.demo.ui.chat

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.clearAndSetSemantics
import dev.clawseed.demo.ui.components.GlassContainer
import dev.clawseed.demo.ui.components.GlassLevel
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.components.ClawSeedCard
import dev.clawseed.demo.ui.components.ClawSeedSheetHeader
import androidx.compose.material3.HorizontalDivider
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.persona.PersonaDot
import dev.clawseed.demo.ui.persona.personaCardColor
import dev.clawseed.demo.ui.persona.personaSummary
import dev.clawseed.sdk.android.ClawSeedAndroid
import dev.clawseed.sdk.core.model.PersonaInfo

/**
 * A persona entry that starts a chat when tapped. The default assistant is an
 * explicit persona named `default`, just like every other chat target.
 */
private data class PickerEntry(
    val key: String,
    val targetName: String,
    val displayName: String,
    val subtitle: String?,
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PersonaPickerSheet(
    onDismiss: () -> Unit,
    defaultPersonaName: String? = null,
    onStart: (persona: String?) -> Unit,
) {
    var loading by remember { mutableStateOf(true) }
    var personas by remember { mutableStateOf<List<PersonaInfo>>(emptyList()) }
    var loadError by remember { mutableStateOf(false) }
    var starting by remember { mutableStateOf(false) }

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

    BackHandler(onBack = onDismiss)
    Box(Modifier.fillMaxSize()) {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.32f))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onDismiss)
            .clearAndSetSemantics {})
        GlassContainer(
            modifier = Modifier.align(Alignment.BottomCenter).statusBarsPadding().fillMaxWidth(),
            level = GlassLevel.FloatingInput,
            shape = ClawSeedShapes.Sheet,
        ) {
        Column(modifier = Modifier.navigationBarsPadding().padding(horizontal = 24.dp).padding(bottom = 24.dp)) {
            var dragDistance by remember { mutableStateOf(0f) }
            Box(Modifier.fillMaxWidth().pointerInput(onDismiss) {
                detectVerticalDragGestures(
                    onDragEnd = { if (dragDistance > 80.dp.toPx()) onDismiss(); dragDistance = 0f },
                    onDragCancel = { dragDistance = 0f },
                    onVerticalDrag = { _, amount -> dragDistance += amount },
                )
            }) {
                ClawSeedSheetHeader(stringResource(R.string.persona_new_chat_title), onDismiss)
            }
            HorizontalDivider(Modifier.padding(vertical = 12.dp), color = MaterialTheme.colorScheme.outlineVariant)

            val entries = buildList {
                val defaultTarget = defaultPersonaName ?: "default"
                val defaultSubtitle = if (defaultTarget == "default") {
                    stringResource(R.string.persona_default_desc)
                } else {
                    stringResource(R.string.persona_default_current, defaultTarget)
                }
                add(PickerEntry("default", defaultTarget, stringResource(R.string.persona_default), defaultSubtitle))
                personas.filterNot { it.name == "default" }
                    .forEach { add(PickerEntry(it.name, it.name, it.subtitle(), it.subtitle())) }
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
                    modifier = Modifier.weight(1f, fill = false).heightIn(max = 360.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(entries, key = { it.key }) { entry ->
                        val persona = personas.find { it.name == entry.targetName }
                        ClawSeedCard(
                            containerColor = persona?.let { personaCardColor(it.name, it.color) } ?: Color.Unspecified,
                            onClick = {
                                if (!starting) {
                                    starting = true
                                    onStart(entry.targetName)
                                }
                            },
                        ) {
                            Row(
                                modifier = Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(12.dp),
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                if (persona != null) {
                                    PersonaDot(persona.name, Modifier.size(36.dp), showInitial = true,
                                        avatar = persona.avatar, color = persona.color)
                                }
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(
                                        text = entry.displayName,
                                        style = MaterialTheme.typography.bodyLarge,
                                    )
                                    val sub = entry.subtitle ?: stringResource(R.string.persona_default_desc)
                                    if (sub.isNotEmpty()) {
                                        Text(
                                            text = sub,
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                            maxLines = 2,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                }
                                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
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

        }
        }
    }
}

/** One-line summary of a persona's overrides, for the picker subtitle. */
private fun PersonaInfo.subtitle(): String {
    return personaSummary(this)
}
