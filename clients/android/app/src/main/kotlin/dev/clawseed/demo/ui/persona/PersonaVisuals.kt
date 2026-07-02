package dev.clawseed.demo.ui.persona

import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

private val PersonaPalette = listOf(
    Color(0xFF2563EB),
    Color(0xFF0F766E),
    Color(0xFF7C3AED),
    Color(0xFFDB2777),
    Color(0xFFEA580C),
    Color(0xFF16A34A),
    Color(0xFF0891B2),
    Color(0xFFC026D3),
    Color(0xFFCA8A04),
    Color(0xFF4F46E5),
)

@Composable
fun personaAccentColor(name: String): Color {
    return PersonaPalette[Math.floorMod(name.hashCode(), PersonaPalette.size)]
}

@Composable
fun personaContainerColor(name: String): Color {
    val alpha = if (isSystemInDarkTheme()) 0.18f else 0.11f
    return personaAccentColor(name).copy(alpha = alpha)
}

@Composable
fun personaContentColor(name: String): Color {
    return personaAccentColor(name)
}

@Composable
fun PersonaDot(
    name: String,
    modifier: Modifier = Modifier.size(10.dp),
    showInitial: Boolean = false,
) {
    val color = personaAccentColor(name)
    val contentColor = if (color.luminance() > 0.55f) Color(0xFF1C1B1F) else Color.White
    Box(
        modifier = modifier
            .clip(CircleShape)
            .background(color),
        contentAlignment = Alignment.Center,
    ) {
        if (showInitial) {
            Text(
                text = personaInitial(name),
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
                color = contentColor,
                textAlign = TextAlign.Center,
            )
        }
    }
}

private fun personaInitial(name: String): String {
    return name.trim().firstOrNull()?.uppercase() ?: "?"
}
