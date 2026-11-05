package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import dev.clawseed.demo.ui.theme.ClawSeedShapes

@Composable
fun SettingsGroup(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    GlassSurface(modifier, shape = ClawSeedShapes.Card, content = content)
}
