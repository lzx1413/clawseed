package dev.clawseed.demo.ui.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance

private val DarkColors = darkColorScheme(
    primary = Color(0xFFE1BA70),
    onPrimary = Color(0xFF241B0A),
    primaryContainer = Color(0xFF3C3322),
    onPrimaryContainer = Color(0xFFF1D8A6),
    inversePrimary = Color(0xFF785421),
    secondary = Color(0xFFBBC3CF),
    onSecondary = Color(0xFF252C35),
    secondaryContainer = Color(0xFF3C3322),
    onSecondaryContainer = Color(0xFFF1D8A6),
    tertiary = Color(0xFFA7C8CD),
    onTertiary = Color(0xFF17353A),
    tertiaryContainer = Color(0xFF263C41),
    onTertiaryContainer = Color(0xFFCBE6EB),
    background = Color(0xFF111315),
    onBackground = Color(0xFFE7E9ED),
    surface = Color(0xFF17191C),
    onSurface = Color(0xFFE7E9ED),
    surfaceVariant = Color(0xFF24272C),
    onSurfaceVariant = Color(0xFFB7BEC8),
    surfaceDim = Color(0xFF111315),
    surfaceBright = Color(0xFF363A40),
    surfaceContainerLowest = Color(0xFF0D0F11),
    surfaceContainerLow = Color(0xFF1B1E22),
    surfaceContainer = Color(0xFF202328),
    surfaceContainerHigh = Color(0xFF292D33),
    surfaceContainerHighest = Color(0xFF32373E),
    surfaceTint = Color.Transparent,
    inverseSurface = Color(0xFFE3E6EB),
    inverseOnSurface = Color(0xFF292D33),
    outline = Color(0xFF7B838F),
    outlineVariant = Color(0xFF3B414A),
    error = Color(0xFFF28C82),
    onError = Color(0xFF4A1713),
    errorContainer = Color(0xFF482521),
    onErrorContainer = Color(0xFFFFDAD5),
    scrim = Color.Black,
)

private val LightColors = lightColorScheme(
    primary = Color(0xFF7C591F),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFF1E4C8),
    onPrimaryContainer = Color(0xFF382A12),
    inversePrimary = Color(0xFFE1BA70),
    secondary = Color(0xFF515D6D),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFF1E4C8),
    onSecondaryContainer = Color(0xFF382A12),
    tertiary = Color(0xFF365F66),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFDCECEF),
    onTertiaryContainer = Color(0xFF1B3B41),
    background = Color(0xFFF7F8FA),
    onBackground = Color(0xFF20242A),
    surface = Color(0xFFFAFBFC),
    onSurface = Color(0xFF20242A),
    surfaceVariant = Color(0xFFEBEEF2),
    onSurfaceVariant = Color(0xFF535D6B),
    surfaceDim = Color(0xFFDCE0E6),
    surfaceBright = Color.White,
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFF3F5F7),
    surfaceContainer = Color(0xFFEEF0F3),
    surfaceContainerHigh = Color(0xFFE7EAEF),
    surfaceContainerHighest = Color(0xFFE0E4EA),
    surfaceTint = Color.Transparent,
    inverseSurface = Color(0xFF292D33),
    inverseOnSurface = Color(0xFFE7E9ED),
    outline = Color(0xFF727B88),
    outlineVariant = Color(0xFFCDD3DC),
    error = Color(0xFFB3261E),
    onError = Color.White,
    errorContainer = Color(0xFFFFE3DE),
    onErrorContainer = Color(0xFF681D17),
    scrim = Color.Black,
)

internal fun appColorScheme(dark: Boolean, oled: Boolean = false): ColorScheme = when {
    !dark -> LightColors
    oled -> DarkColors.copy(background = Color.Black, surface = Color.Black, surfaceDim = Color.Black)
    else -> DarkColors
}

// Success is separate from selection (gold) and informational accents (blue-gray).
internal val ColorScheme.success: Color
    get() = if (surface.luminance() < 0.5f) Color(0xFF8ACBA7) else Color(0xFF246B47)
