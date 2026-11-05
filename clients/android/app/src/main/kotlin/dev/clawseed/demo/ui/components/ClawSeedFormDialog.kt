package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import dev.clawseed.demo.ui.theme.ClawSeedShapes

/** Forms keep their actions visible while the fields scroll within the available IME-safe space. */
@Composable
fun ClawSeedFormDialog(
    onDismissRequest: () -> Unit,
    title: @Composable () -> Unit,
    text: @Composable () -> Unit,
    confirmButton: @Composable () -> Unit,
    dismissButton: @Composable () -> Unit = {},
) {
    Dialog(onDismissRequest = onDismissRequest,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        Box(Modifier.fillMaxSize()) {
            // The fullscreen window needs an explicit outside-tap target; Surface blocks
            // taps within the form, preserving the original dialog dismissal behavior.
            Box(Modifier.matchParentSize().clickable(
                interactionSource = remember { MutableInteractionSource() }, indication = null,
                onClick = onDismissRequest,
            ).clearAndSetSemantics {})
            BoxWithConstraints(
                Modifier.fillMaxSize().safeDrawingPadding().imePadding().padding(16.dp),
                contentAlignment = Alignment.Center,
            ) {
                val availableHeight = maxHeight
                Surface(Modifier.widthIn(max = 560.dp).fillMaxWidth().heightIn(max = maxHeight),
                    shape = ClawSeedShapes.Card, color = MaterialTheme.colorScheme.surface,
                    contentColor = MaterialTheme.colorScheme.onSurface) {
                    Column {
                        Box(Modifier.heightIn(max = availableHeight * 0.3f).verticalScroll(rememberScrollState())
                            .padding(horizontal = 20.dp, vertical = 16.dp)) {
                            ProvideTextStyle(MaterialTheme.typography.titleLarge, title)
                        }
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState())
                            .padding(20.dp)) { text() }
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        FlowRow(Modifier.fillMaxWidth().heightIn(max = availableHeight * 0.45f)
                            .verticalScroll(rememberScrollState()).padding(12.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                            verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            dismissButton()
                            confirmButton()
                        }
                    }
                }
            }
        }
    }
}
