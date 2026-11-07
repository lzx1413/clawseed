package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.clawseed.demo.ui.theme.pageTitle

/** Page titles may grow with the user's font size instead of clipping inside a fixed bar. */
@Composable
fun ClawSeedTopAppBar(
    title: @Composable () -> Unit,
    navigationIcon: @Composable () -> Unit = {},
    actions: @Composable RowScope.() -> Unit = {},
) {
    Surface(color = MaterialTheme.colorScheme.surface, contentColor = MaterialTheme.colorScheme.onSurface) {
        Row(
            Modifier.fillMaxWidth()
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
                .heightIn(min = 64.dp).padding(horizontal = 4.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            navigationIcon()
            Box(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                ProvideTextStyle(MaterialTheme.typography.pageTitle, title)
            }
            Row(verticalAlignment = Alignment.CenterVertically, content = actions)
        }
    }
}
