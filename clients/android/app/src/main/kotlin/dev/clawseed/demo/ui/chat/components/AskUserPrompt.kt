package dev.clawseed.demo.ui.chat.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.R
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

    Surface(
        modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp),
        shape = RoundedCornerShape(8.dp),
        tonalElevation = 2.dp,
        border = androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(request.question, style = MaterialTheme.typography.titleSmall)
            when (request.kind) {
                "single_select" -> request.options.forEach { option ->
                    OptionRow(
                        option = option,
                        selected = selected == option.id,
                        multiple = false,
                        enabled = controlsEnabled,
                        onClick = { selected = option.id },
                    )
                }
                "multi_select" -> request.options.forEach { option ->
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
                "text" -> OutlinedTextField(
                    value = text,
                    onValueChange = { if (it.length <= 1000) text = it },
                    enabled = controlsEnabled,
                    placeholder = request.placeholder?.let { { Text(it) } },
                    modifier = Modifier.fillMaxWidth(),
                    minLines = 1,
                    maxLines = 4,
                )
            }
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (submitting) {
                    CircularProgressIndicator(
                        modifier = Modifier.padding(end = 12.dp).size(20.dp),
                        strokeWidth = 2.dp,
                    )
                }
                TextButton(
                    onClick = { onAnswer(if (request.kind == "confirm") "declined" else "cancelled", null) },
                    enabled = controlsEnabled,
                ) {
                    Text(stringResource(if (request.kind == "confirm") R.string.ask_user_decline else R.string.common_cancel))
                }
                Button(
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
    Row(
        modifier = Modifier.fillMaxWidth().clickable(enabled = enabled, onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (multiple) Checkbox(selected, onCheckedChange = null, enabled = enabled)
        else RadioButton(selected, onClick = null, enabled = enabled)
        Column(modifier = Modifier.padding(start = 6.dp)) {
            Text(option.label, style = MaterialTheme.typography.bodyMedium)
            option.description?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}
