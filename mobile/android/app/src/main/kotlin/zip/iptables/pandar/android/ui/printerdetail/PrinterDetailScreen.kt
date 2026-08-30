package zip.iptables.pandar.android.ui.printerdetail

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.domain.model.AmsTray
import zip.iptables.pandar.android.domain.model.Materials
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.domain.model.PrinterAxis
import zip.iptables.pandar.android.domain.model.PrinterControlIntent
import zip.iptables.pandar.android.ui.components.StatusPill

@Composable
fun PrinterDetailScreen(
    state: PrinterDetailUiState,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onStop: () -> Unit,
    onToggleLight: () -> Unit,
    onHome: () -> Unit,
    onMoveAxis: (PrinterAxis, Double) -> Unit,
    onSetChamberLight: (Boolean) -> Unit,
    onSetHotend: (Int) -> Unit,
    onSetBed: (Int) -> Unit,
    onSetChamber: (Int) -> Unit,
    onAmsLoad: (PrinterControlIntent.AmsLoadFilament) -> Unit,
    onAmsUnload: (PrinterControlIntent.AmsUnloadFilament) -> Unit,
    onAmsReread: (PrinterControlIntent.AmsRereadRfid) -> Unit,
) {
    val printer = state.printer
    Scaffold { padding ->
        if (printer == null) {
            Column(Modifier.padding(padding).fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                Text(state.error ?: "Loading…", style = MaterialTheme.typography.bodyMedium)
            }
            return@Scaffold
        }
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Header(printer)
            HorizontalDivider()
            PrintActionsRow(state, onPause, onResume, onStop, onToggleLight)
            HorizontalDivider()
            AxisControls(
                enabled = !state.inFlight,
                onHome = onHome,
                onMoveAxis = onMoveAxis,
            )
            HorizontalDivider()
            TemperatureControls(printer, state, onSetHotend, onSetBed, onSetChamber)
            HorizontalDivider()
            ChamberLightControl(printer, state, onSetChamberLight)
            HorizontalDivider()
            MaterialsSection(printer.materials, printer, state, onAmsLoad, onAmsUnload, onAmsReread)
            state.toast?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        }
    }
}

@Composable
private fun Header(printer: Printer) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(printer.name, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            StatusPill(printer.status)
        }
        printer.model?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        Text(printer.serialNumber, style = MaterialTheme.typography.bodyMedium, fontFamily = FontFamily.Monospace)
    }
}

@Composable
private fun PrintActionsRow(
    state: PrinterDetailUiState,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onStop: () -> Unit,
    onToggleLight: () -> Unit,
) {
    Column {
        Text("Print actions", style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.padding(4.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onPause, enabled = !state.inFlight) { Text("Pause") }
            OutlinedButton(onResume, enabled = !state.inFlight) { Text("Resume") }
            Button(onStop, enabled = !state.inFlight) { Text("Stop") }
            OutlinedButton(onToggleLight, enabled = !state.inFlight) { Text("Toggle light") }
        }
    }
}

@Composable
private fun TemperatureControls(
    printer: Printer,
    state: PrinterDetailUiState,
    onSetHotend: (Int) -> Unit,
    onSetBed: (Int) -> Unit,
    onSetChamber: (Int) -> Unit,
) {
    Column {
        Text("Temperatures", style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.padding(4.dp))
        printer.nozzleTemperatures.forEach { nozzle ->
            val label = nozzle.label?.let { "Nozzle $it" } ?: "Nozzle"
            TemperatureRow(label, nozzle.currentCelsius, nozzle.targetCelsius)
        }
        TemperatureRow("Bed", printer.bedTemperatureCelsius, printer.bedTargetTemperatureCelsius)
        TemperatureRow("Chamber", printer.chamberTemperatureCelsius, null)
        Spacer(Modifier.padding(4.dp))
        SetTemperatureControl(label = "Set hotend (°C)", enabled = !state.inFlight, onApply = onSetHotend)
        SetTemperatureControl(label = "Set bed (°C)", enabled = !state.inFlight, onApply = onSetBed)
        SetTemperatureControl(label = "Set chamber (°C)", enabled = !state.inFlight, onApply = onSetChamber)
    }
}

@Composable
private fun TemperatureRow(label: String, current: String?, target: String?) {
    val text = buildString {
        append("$label: ")
        append(current ?: "—")
        if (target != null) append(" / target $target")
    }
    Text(text, style = MaterialTheme.typography.bodyMedium)
}

@Composable
private fun SetTemperatureControl(label: String, enabled: Boolean, onApply: (Int) -> Unit) {
    var value by remember { mutableStateOf("") }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = value,
            onValueChange = { value = it.filter(Char::isDigit) },
            label = { Text(label) },
            singleLine = true,
            modifier = Modifier.weight(1f),
        )
        Button(
            onClick = { value.toIntOrNull()?.let(onApply) },
            enabled = enabled && value.toIntOrNull() != null,
        ) { Text("Apply") }
    }
}

@Composable
private fun ChamberLightControl(printer: Printer, state: PrinterDetailUiState, onSetChamberLight: (Boolean) -> Unit) {
    val current = printer.chamberLightOn ?: return
    Column {
        Text("Chamber light", style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.padding(4.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(if (current) "On" else "Off", modifier = Modifier.weight(1f))
            Switch(
                checked = current,
                onCheckedChange = { on -> onSetChamberLight(on) },
                enabled = !state.inFlight,
            )
        }
    }
}

@Composable
private fun MaterialsSection(
    materials: Materials?,
    printer: Printer,
    state: PrinterDetailUiState,
    onAmsLoad: (PrinterControlIntent.AmsLoadFilament) -> Unit,
    onAmsUnload: (PrinterControlIntent.AmsUnloadFilament) -> Unit,
    onAmsReread: (PrinterControlIntent.AmsRereadRfid) -> Unit,
) {
    Column {
        Text("Materials", style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.padding(4.dp))
        if (materials == null) {
            Text("No material data", style = MaterialTheme.typography.bodySmall)
            return
        }
        materials.amsUnits.forEach { unit ->
            Text("AMS ${unit.unitId ?: "?"}", style = MaterialTheme.typography.labelLarge)
            unit.trays.forEach { tray ->
                AmsTrayRow(
                    tray = tray,
                    amsUnitId = unit.unitId,
                    enabled = !state.inFlight,
                    active = isActiveTray(printer, unit.unitId, tray),
                    onAmsLoad = onAmsLoad,
                    onAmsUnload = onAmsUnload,
                    onAmsReread = onAmsReread,
                )
            }
        }
        if (materials.externalSpools.isNotEmpty()) {
            Spacer(Modifier.padding(4.dp))
            Text("External spools", style = MaterialTheme.typography.labelLarge)
            materials.externalSpools.forEach { spool ->
                val spoolActive = isActiveExternalTray(printer, spool.externalId, spool.globalTrayId)
                Row(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    spool.color?.let { ColorSwatch(it) }
                    Column(modifier = Modifier.weight(1f)) {
                        Text(spool.type ?: "Unknown", style = MaterialTheme.typography.bodyMedium)
                        Text(spool.color ?: "—", style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
                        spool.name?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
                        spool.kValue?.let { Text("k $it", style = MaterialTheme.typography.labelSmall) }
                        if (spoolActive) ActiveLabel()
                    }
                    // The hub's ams_load/unload_filament actions require ams_id + slot_id, which
                    // external spools do not have; external spools are display-only here.
                    Text("Display only", style = MaterialTheme.typography.labelSmall)
                }
            }
        }
    }
}

@Composable
private fun AmsTrayRow(
    tray: AmsTray,
    amsUnitId: String?,
    enabled: Boolean,
    active: Boolean,
    onAmsLoad: (PrinterControlIntent.AmsLoadFilament) -> Unit,
    onAmsUnload: (PrinterControlIntent.AmsUnloadFilament) -> Unit,
    onAmsReread: (PrinterControlIntent.AmsRereadRfid) -> Unit,
) {
    val amsId = amsUnitId?.toIntOrNull()
    val slotId = tray.trayId?.toIntOrNull()
    val actionable = amsId != null && slotId != null && enabled
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        tray.color?.let { ColorSwatch(it) }
        Column(modifier = Modifier.weight(1f)) {
            Text(tray.type ?: "Unknown", style = MaterialTheme.typography.bodyMedium)
            Text(tray.color ?: "—", style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace)
            tray.remainingEstimate?.let { Text("$it left", style = MaterialTheme.typography.labelSmall) }
            tray.kValue?.let { Text("k $it", style = MaterialTheme.typography.labelSmall) }
            if (active) ActiveLabel()
        }
        if (actionable) {
            OutlinedButton(onClick = { onAmsLoad(PrinterControlIntent.AmsLoadFilament(amsId = amsId!!, slotId = slotId!!, globalTrayId = tray.globalTrayId)) }) { Text("Load") }
            OutlinedButton(onClick = { onAmsUnload(PrinterControlIntent.AmsUnloadFilament(amsId = amsId!!, slotId = slotId!!, globalTrayId = tray.globalTrayId)) }) { Text("Unload") }
            OutlinedButton(onClick = { onAmsReread(PrinterControlIntent.AmsRereadRfid(amsId = amsId!!, slotId = slotId!!)) }) { Text("Reread") }
        } else if (amsId == null || slotId == null) {
            Text("No slot id", style = MaterialTheme.typography.labelSmall)
        }
    }
}

private fun isActiveTray(printer: Printer, amsUnitId: String?, tray: AmsTray): Boolean {
    val active = printer.materials?.activeTray ?: return false
    val activeAms = active.amsId
    val activeTrayId = active.trayId
    val activeGlobal = active.globalTrayId
    return (activeAms != null && activeAms == amsUnitId && activeTrayId != null && activeTrayId == tray.trayId) ||
        (activeGlobal != null && activeGlobal == tray.globalTrayId)
}

private fun isActiveExternalTray(printer: Printer, externalId: String?, globalTrayId: Int?): Boolean {
    val active = printer.materials?.activeTray ?: return false
    val activeExternal = active.externalId
    val activeGlobal = active.globalTrayId
    return (activeExternal != null && activeExternal == externalId) ||
        (activeGlobal != null && globalTrayId != null && activeGlobal == globalTrayId)
}

@Composable
private fun ActiveLabel() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(
            imageVector = Icons.Filled.Check,
            contentDescription = "Active",
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.primary,
        )
        Spacer(Modifier.size(4.dp))
        Text("Active", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun ColorSwatch(hex: String) {
    val color = remember(hex) {
        try {
            val normalized = hex.removePrefix("#")
            Color(java.lang.Long.parseLong(normalized, 16).toInt() or 0xFF000000.toInt())
        } catch (_: Throwable) {
            Color.Gray
        }
    }
    Box(
        modifier = Modifier
            .size(20.dp)
            .background(color, RoundedCornerShape(4.dp)),
    )
}
