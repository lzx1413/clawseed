package dev.clawseed.demo.ui.persona

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage

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
fun personaAccentColor(name: String, colorOverride: String?): Color {
    return parsePersonaColor(colorOverride) ?: personaAccentColor(name)
}

@Composable
fun personaContainerColor(name: String, colorOverride: String? = null): Color {
    return personaAccentColor(name, colorOverride).copy(alpha = 0.12f)
        .compositeOver(MaterialTheme.colorScheme.surfaceContainerLow)
}

@Composable
fun personaContentColor(name: String, colorOverride: String? = null): Color {
    return readablePersonaColor(personaAccentColor(name, colorOverride), personaContainerColor(name, colorOverride))
}

@Composable
fun personaCardColor(name: String, colorOverride: String? = null): Color =
    personaAccentColor(name, colorOverride).copy(alpha = 0.035f)
        .compositeOver(MaterialTheme.colorScheme.surfaceContainerLow)

internal fun readablePersonaColor(accent: Color, background: Color): Color {
    val target = personaInitialColor(background)
    for (step in 0..20) {
        val candidate = lerp(accent, target, step / 20f)
        val lighter = maxOf(candidate.luminance(), background.luminance())
        val darker = minOf(candidate.luminance(), background.luminance())
        if ((lighter + 0.05f) / (darker + 0.05f) >= 4.5f) return candidate
    }
    return target
}

internal fun personaInitialColor(background: Color): Color =
    if (background.luminance() > 0.179f) Color.Black else Color.White

@Composable
fun PersonaDot(
    name: String,
    modifier: Modifier = Modifier.size(10.dp),
    showInitial: Boolean = false,
    avatar: String? = null,
    color: String? = null,
) {
    val accentColor = personaAccentColor(name, color)
    val contentColor = personaInitialColor(accentColor)
    val context = LocalContext.current
    val avatarUri = remember(context, avatar) {
        avatar?.trim()?.takeIf { PersonaAvatarStorage.isAvatarUri(it) }?.let {
            runCatching { PersonaAvatarStorage.resolveAvatarUri(context, it) }.getOrNull()
        }
    }
    var avatarLoaded by remember(avatarUri) { mutableStateOf(false) }
    Box(
        modifier = modifier
            .clip(CircleShape)
            .background(accentColor),
        contentAlignment = Alignment.Center,
    ) {
        val label = avatar
            ?.trim()
            ?.takeIf { it.isNotEmpty() && !it.isLikelyImageUri() }
            ?: if (showInitial) personaInitial(name) else ""
        if (!avatarLoaded && label.isNotEmpty()) {
            Text(
                text = label.take(2),
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
                color = contentColor,
                textAlign = TextAlign.Center,
            )
        }
        if (avatarUri != null) {
            // Coil decodes off the main thread and shares its memory cache
            // across history rows, including rows that leave and re-enter view.
            AsyncImage(
                model = avatarUri,
                onSuccess = { avatarLoaded = true },
                onError = { avatarLoaded = false },
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

private fun String.isLikelyImageUri(): Boolean =
    PersonaAvatarStorage.isAvatarUri(this)

private fun personaInitial(name: String): String {
    return name.trim().firstOrNull()?.uppercase() ?: "?"
}

private fun parsePersonaColor(value: String?): Color? {
    val hex = value?.trim()?.removePrefix("#") ?: return null
    if (hex.length != 6 || !hex.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) {
        return null
    }
    return Color(("FF$hex").toLong(16))
}
