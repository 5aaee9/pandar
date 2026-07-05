package zip.iptables.pandar.android.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.domain.model.Severity
import zip.iptables.pandar.android.domain.status.statusMeta
import zip.iptables.pandar.android.ui.theme.CriticalColor
import zip.iptables.pandar.android.ui.theme.CriticalContainer
import zip.iptables.pandar.android.ui.theme.SuccessColor
import zip.iptables.pandar.android.ui.theme.SuccessContainer
import zip.iptables.pandar.android.ui.theme.WarningColor
import zip.iptables.pandar.android.ui.theme.WarningContainer

@Composable
fun StatusPill(rawStatus: String, modifier: Modifier = Modifier) {
    val meta = statusMeta(rawStatus)
    val icon: ImageVector
    val fg: Color
    val bg: Color
    when (meta.severity) {
        Severity.SUCCESS -> {
            icon = Icons.Default.CheckCircle; fg = SuccessColor; bg = SuccessContainer
        }
        Severity.WARNING -> {
            icon = Icons.Default.Warning; fg = WarningColor; bg = WarningContainer
        }
        Severity.CRITICAL -> {
            icon = Icons.Default.Error; fg = CriticalColor; bg = CriticalContainer
        }
        Severity.INFO -> {
            icon = Icons.Default.Info
            fg = MaterialTheme.colorScheme.onSurface
            bg = MaterialTheme.colorScheme.surfaceVariant
        }
    }
    Row(
        modifier = modifier
            .background(color = bg, shape = RoundedCornerShape(50))
            .padding(horizontal = 10.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = meta.severity.name, tint = fg, modifier = Modifier.size(16.dp))
        Spacer(Modifier.width(6.dp))
        Text(text = meta.label, style = MaterialTheme.typography.labelMedium, color = fg)
    }
}
