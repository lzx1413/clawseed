package dev.clawseed.demo.ui.persona

import android.graphics.BitmapFactory
import android.media.ThumbnailUtils
import android.net.Uri
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
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
fun personaAccentColor(name: String, colorOverride: String?): Color {
    return parsePersonaColor(colorOverride) ?: personaAccentColor(name)
}

@Composable
fun personaContainerColor(name: String, colorOverride: String? = null): Color {
    val alpha = if (isSystemInDarkTheme()) 0.18f else 0.11f
    return personaAccentColor(name, colorOverride).copy(alpha = alpha)
}

@Composable
fun personaContentColor(name: String, colorOverride: String? = null): Color {
    return personaAccentColor(name, colorOverride)
}

@Composable
fun PersonaDot(
    name: String,
    modifier: Modifier = Modifier.size(10.dp),
    showInitial: Boolean = false,
    avatar: String? = null,
    color: String? = null,
) {
    val accentColor = personaAccentColor(name, color)
    val contentColor = if (accentColor.luminance() > 0.55f) Color(0xFF1C1B1F) else Color.White
    val avatarImage = rememberPersonaAvatarBitmap(avatar)
    Box(
        modifier = modifier
            .clip(CircleShape)
            .background(accentColor),
        contentAlignment = Alignment.Center,
    ) {
        if (avatarImage != null) {
            Image(
                bitmap = avatarImage,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            val label = avatar
                ?.trim()
                ?.takeIf { it.isNotEmpty() && !it.isLikelyImageUri() }
                ?: if (showInitial) personaInitial(name) else ""
            if (label.isNotEmpty()) {
                Text(
                    text = label.take(2),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                    color = contentColor,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

@Composable
private fun rememberPersonaAvatarBitmap(avatar: String?) =
    avatar
        ?.trim()
        ?.takeIf { PersonaAvatarStorage.isAvatarUri(it) }
        ?.let { uriText ->
            val context = LocalContext.current
            remember(uriText) {
                runCatching {
                    val uri = PersonaAvatarStorage.resolveAvatarUri(context, uriText)
                        ?: return@runCatching null
                    decodePersonaAvatar(context.contentResolver, uri)?.asImageBitmap()
                }.getOrNull()
            }
        }

private fun decodePersonaAvatar(
    contentResolver: android.content.ContentResolver,
    uri: Uri,
) = run {
    val targetSize = 128
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    contentResolver.openInputStream(uri)?.use { input ->
        BitmapFactory.decodeStream(input, null, bounds)
    }
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return@run null

    val options = BitmapFactory.Options().apply {
        inSampleSize = calculateInSampleSize(bounds.outWidth, bounds.outHeight, targetSize)
    }
    val decoded = contentResolver.openInputStream(uri)?.use { input ->
        BitmapFactory.decodeStream(input, null, options)
    } ?: return@run null
    ThumbnailUtils.extractThumbnail(decoded, targetSize, targetSize)
}

private fun calculateInSampleSize(width: Int, height: Int, targetSize: Int): Int {
    var sampleSize = 1
    var halfWidth = width / 2
    var halfHeight = height / 2
    while (halfWidth / sampleSize >= targetSize && halfHeight / sampleSize >= targetSize) {
        sampleSize *= 2
    }
    return sampleSize
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
