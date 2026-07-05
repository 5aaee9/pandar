package zip.iptables.pandar.android.ui.printers

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.ui.components.StatusPill

@Composable
fun PrinterCard(printer: Printer, onClick: () -> Unit) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick),
    ) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(printer.name, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                StatusPill(printer.status)
            }
            printer.model?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            Text(
                printer.serialNumber,
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = FontFamily.Monospace,
            )
            TemperatureRow(label = "Bed", current = printer.bedTemperatureCelsius, target = printer.bedTargetTemperatureCelsius)
            ActiveNozzleRow(printer)
            ChamberLightRow(printer)
        }
    }
}

@Composable
private fun TemperatureRow(label: String, current: String?, target: String?) {
    val text = buildString {
        append(label)
        append(": ")
        append(current ?: "—")
        if (target != null) append(" / $target")
    }
    Text(text, style = MaterialTheme.typography.bodyMedium)
}

@Composable
private fun ActiveNozzleRow(printer: Printer) {
    val active = printer.activeNozzle?.let { id ->
        printer.nozzleTemperatures.firstOrNull { it.label == id }
    }
    if (active != null) {
        TemperatureRow(label = "Nozzle", current = active.currentCelsius, target = active.targetCelsius)
    }
}

@Composable
private fun ChamberLightRow(printer: Printer) {
    printer.chamberLightOn?.let { on ->
        Text("Light: ${if (on) "On" else "Off"}", style = MaterialTheme.typography.bodySmall)
    }
}
