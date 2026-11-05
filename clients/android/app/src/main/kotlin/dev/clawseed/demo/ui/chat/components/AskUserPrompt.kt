package dev.clawseed.demo.ui.chat.components

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import dev.clawseed.demo.ui.components.ClawSeedButton
import dev.clawseed.demo.ui.components.ClawSeedTextButton
import dev.clawseed.demo.ui.components.ClawSeedTextField
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.res.stringResource
import dev.clawseed.demo.R
import dev.clawseed.demo.ui.components.ClawSeedChoiceRow
import dev.clawseed.demo.ui.components.GlassLevel
import dev.clawseed.demo.ui.components.GlassSurface
import dev.clawseed.demo.ui.theme.ClawSeedShapes
import dev.clawseed.demo.ui.theme.ClawSeedShadows
import dev.clawseed.demo.ui.theme.ClawSeedSpacing
import dev.clawseed.sdk.core.model.ChatEvent
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray

@Composable
fun AskUserPrompt(
    request: ChatEvent.AskUserRequested,
    submitting: Boolean,
    enabled: Boolean,
    onAnswer: (status: String, answer: JsonElement?) -> Unit,
    modifier: Modifier = Modifier,
) {
    var selected by remember(request.requestId) { mutableStateOf<String?>(null) }
    var selectedMany by remember(request.requestId) { mutableStateOf(emptySet<String>()) }
    var text by remember(request.requestId) { mutableStateOf("") }
    val controlsEnabled = enabled && !submitting

    GlassSurface(
        modifier = modifier.fillMaxWidth()
            .padding(horizontal = ClawSeedSpacing.Medium, vertical = ClawSeedSpacing.Small)
            .dropShadow(ClawSeedShapes.Card, ClawSeedShadows.FloatingControl),
        shape = ClawSeedShapes.Card,
        level = GlassLevel.Chrome,
    ) {
        Column(
            modifier = Modifier.padding(ClawSeedSpacing.Page),
            verticalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Medium),
        ) {
            Text(request.question, style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.semantics { heading() })
            when (request.kind) {
                "single_select" -> Column(
                    modifier = Modifier.selectableGroup(),
                    verticalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Small),
                ) {
                    request.options.forEach { option ->
                        OptionRow(
                            option = option,
                            selected = selected == option.id,
                            multiple = false,
                            enabled = controlsEnabled,
                            onClick = { selected = option.id },
                        )
                    }
                }
                "multi_select" -> Column(verticalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Small)) {
                    request.options.forEach { option ->
                        OptionRow(
                            option = option,
                            selected = option.id in selectedMany,
                            multiple = true,
                            enabled = controlsEnabled,
                            onClick = {
                                selectedMany = if (option.id in selectedMany) selectedMany - option.id
                                else selectedMany + option.id
                            },
                        )
                    }
                }
                "text" -> ClawSeedTextField(
                    value = text,
                    onValueChange = { if (it.length <= 1000) text = it },
                    enabled = controlsEnabled,
                    label = { Text(stringResource(R.string.ask_user_answer)) },
                    placeholder = request.placeholder?.let { { Text(it) } },
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 1,
                    maxLines = 4,
                )
            }
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Small, Alignment.End),
                verticalArrangement = Arrangement.spacedBy(ClawSeedSpacing.Small),
            ) {
                ClawSeedTextButton(
                    onClick = { onAnswer(if (request.kind == "confirm") "declined" else "cancelled", null) },
                    enabled = controlsEnabled,
                ) {
                    Text(stringResource(if (request.kind == "confirm") R.string.ask_user_decline else R.string.common_cancel))
                }
                ClawSeedButton(
                    loading = submitting,
                    onClick = {
                        val answer = when (request.kind) {
                            "confirm" -> JsonPrimitive(true)
                            "single_select" -> selected?.let(::JsonPrimitive)
                            "multi_select" -> buildJsonArray { selectedMany.sorted().forEach { add(JsonPrimitive(it)) } }
                            "text" -> JsonPrimitive(text)
                            else -> null
                        }
                        onAnswer("accepted", answer)
                    },
                    enabled = controlsEnabled && when (request.kind) {
                        "single_select" -> selected != null
                        else -> request.kind in setOf("confirm", "multi_select", "text")
                    },
                ) {
                    Text(stringResource(if (request.kind == "confirm") R.string.common_confirm else R.string.ask_user_submit))
                }
            }
        }
    }
}

@Composable
private fun OptionRow(
    option: ChatEvent.AskUserOption,
    selected: Boolean,
    multiple: Boolean,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    ClawSeedChoiceRow(
        title = option.label,
        subtitle = option.description?.takeIf { it.isNotBlank() },
        selected = selected,
        multiple = multiple,
        enabled = enabled,
        onClick = onClick,
    )
}
