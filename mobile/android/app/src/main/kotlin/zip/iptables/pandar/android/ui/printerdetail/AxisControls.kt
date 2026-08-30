package zip.iptables.pandar.android.ui.printerdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.domain.model.PrinterAxis

@Composable
internal fun AxisControls(
    enabled: Boolean,
    onHome: () -> Unit,
    onMoveAxis: (PrinterAxis, Double) -> Unit,
) {
    var confirmHome by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Move axes", style = MaterialTheme.typography.titleMedium)
        PrinterAxis.entries.forEach { axis ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(axis.name, modifier = Modifier.width(20.dp), fontWeight = FontWeight.SemiBold)
                listOf(-10.0, -1.0, 1.0, 10.0).forEach { deltaMm ->
                    val signed = if (deltaMm > 0) "+${deltaMm.toInt()}" else deltaMm.toInt().toString()
                    OutlinedButton(
                        onClick = { onMoveAxis(axis, deltaMm) },
                        enabled = enabled,
                        modifier = Modifier
                            .weight(1f)
                            .semantics {
                                contentDescription = "Move ${axis.name} by $signed mm"
                            },
                    ) {
                        Text(signed)
                    }
                }
            }
        }
        OutlinedButton(
            onClick = { confirmHome = true },
            enabled = enabled,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Home all axes")
        }
    }
    if (confirmHome) {
        AlertDialog(
            onDismissRequest = { confirmHome = false },
            title = { Text("Auto homing") },
            text = { Text("Are you sure you want to trigger auto homing?") },
            confirmButton = {
                TextButton(onClick = {
                    confirmHome = false
                    onHome()
                }) { Text("Homing") }
            },
            dismissButton = {
                TextButton(onClick = { confirmHome = false }) { Text("Cancel") }
            },
        )
    }
}
