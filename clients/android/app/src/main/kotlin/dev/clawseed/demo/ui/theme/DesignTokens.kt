package dev.clawseed.demo.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

object ClawSeedColors {
    val Ink = Color(0xFF101820)
    val Mist = Color(0xFFE9F1F5)
    val GlassLight = Color.White.copy(alpha = 0.62f)
    val GlassDark = Color(0xFF18242C).copy(alpha = 0.72f)
    val HairlineLight = Color.White.copy(alpha = 0.34f)
    val HairlineDark = Color.White.copy(alpha = 0.12f)
    val Accent = Color(0xFF76AFC4)
    val Positive = Color(0xFF65B89A)
    val Warning = Color(0xFFE0AA63)
    val Danger = Color(0xFFD87878)
}

object ClawSeedShapes {
    val Small = RoundedCornerShape(10.dp)
    val Medium = RoundedCornerShape(16.dp)
    val Large = RoundedCornerShape(24.dp)
    val Sheet = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)
}
