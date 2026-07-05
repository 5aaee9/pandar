package zip.iptables.pandar.android.ui.theme

import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

// Neutral palette derived from DESIGN.md (Pandar OKLCH tokens, approximated to sRGB).
private val InkLight = Color(0xFF0A0A0A)
private val InkDark = Color(0xFFF5F5F5)
private val SurfaceLight = Color(0xFFFFFFFF)
private val SurfaceDark = Color(0xFF2A2A2A)
private val BackgroundDark = Color(0xFF1A1A1A)
private val SecondarySurfaceLight = Color(0xFFF4F4F5)
private val SecondarySurfaceDark = Color(0xFF3A3A3A)
private val BorderLight = Color(0xFFE4E4E7)
private val BorderDark = Color(0xFF4A4A4A)

// Semantic status colors (always paired with icon + label, never color alone).
val CriticalColor = Color(0xFFB91C1C)
val CriticalContainer = Color(0xFFFEE2E2)
val WarningColor = Color(0xFFB45309)
val WarningContainer = Color(0xFFFEF3C7)
val SuccessColor = Color(0xFF15803D)
val SuccessContainer = Color(0xFFDCFCE7)

val PandarLightColors = lightColorScheme(
    primary = InkLight,
    onPrimary = Color.White,
    secondary = SecondarySurfaceLight,
    onSecondary = InkLight,
    background = SurfaceLight,
    onBackground = InkLight,
    surface = SurfaceLight,
    onSurface = InkLight,
    surfaceVariant = SecondarySurfaceLight,
    onSurfaceVariant = InkLight,
    outline = BorderLight,
    outlineVariant = BorderLight,
    error = CriticalColor,
)

val PandarDarkColors = darkColorScheme(
    primary = InkDark,
    onPrimary = InkLight,
    secondary = SecondarySurfaceDark,
    onSecondary = InkDark,
    background = BackgroundDark,
    onBackground = InkDark,
    surface = SurfaceDark,
    onSurface = InkDark,
    surfaceVariant = SecondarySurfaceDark,
    onSurfaceVariant = InkDark,
    outline = BorderDark,
    outlineVariant = BorderDark,
    error = Color(0xFFF87171),
)
