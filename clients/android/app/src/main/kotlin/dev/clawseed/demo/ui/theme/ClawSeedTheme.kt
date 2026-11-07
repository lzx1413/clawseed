package dev.clawseed.demo.ui.theme

import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.dp

private val defaults = Typography()
val ClawSeedTypography = Typography(
    bodyLarge = defaults.bodyLarge.copy(fontFamily = FontFamily.Default, fontSize = 16.sp),
    bodyMedium = defaults.bodyMedium.copy(fontFamily = FontFamily.Default, fontSize = 14.sp),
)

val Typography.pageTitle: TextStyle get() = titleLarge
val Typography.sectionTitle: TextStyle get() = titleMedium
val Typography.rowTitle: TextStyle get() = bodyMedium
val Typography.supporting: TextStyle get() = bodySmall
val Typography.metadata: TextStyle get() = labelSmall

@Composable
fun ClawSeedTheme(dark: Boolean = false, oled: Boolean = false, content: @Composable () -> Unit) {
    // Compose animation clocks already honor the system animator duration scale.
    MaterialTheme(
        colorScheme = appColorScheme(dark, oled),
        typography = ClawSeedTypography,
        shapes = Shapes(ClawSeedShapes.Compact, ClawSeedShapes.Small, ClawSeedShapes.Field,
            ClawSeedShapes.Card, ClawSeedShapes.Input),
        content = content,
    )
}
