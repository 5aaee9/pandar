package zip.iptables.pandar.android.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontFamily

/** Material3 typography using the system sans family (close to Inter). */
val PandarTypography = Typography()

/** Monospace family reserved for machine identifiers (serial numbers, ids, job codes). */
val MonoFontFamily: FontFamily
    @Composable get() = FontFamily.Monospace
