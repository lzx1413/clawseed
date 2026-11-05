package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import dev.clawseed.demo.ui.theme.ClawSeedShapes

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ClawSeedBottomSheet(onDismissRequest: () -> Unit, sheetState: SheetState,
    content: @Composable ColumnScope.() -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismissRequest, sheetState = sheetState,
        shape = ClawSeedShapes.Sheet,
        containerColor = glassFill(MaterialTheme.colorScheme, GlassLevel.Sheet), content = content)
}
