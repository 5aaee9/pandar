package zip.iptables.pandar.android.data.repository

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Printer

class PrinterStateStoreTest {
    @Test
    fun `reconnect snapshots update retained repository state without clearing it`() {
        val store = PrinterStateStore()
        store.apply(PrinterStateUpdate.PrinterSnapshot(printer(status = "idle")))

        assertEquals("idle", store.printers.value.single().status)

        store.apply(PrinterStateUpdate.PrinterSnapshot(printer(status = "running")))

        assertEquals(1, store.printers.value.size)
        assertEquals("running", store.printers.value.single().status)
    }

    @Test
    fun `repository projects command results by printer without transport DTOs`() {
        val store = PrinterStateStore()
        store.apply(PrinterStateUpdate.CommandResult(command(printerId = "printer-1")))
        store.apply(PrinterStateUpdate.CommandResult(command(id = "tenant-command", printerId = null)))

        assertEquals("command-1", store.latestCommandsByPrinter.value["printer-1"]?.id)
        assertFalse(store.latestCommandsByPrinter.value.containsKey("tenant-command"))
    }

    private fun printer(status: String) = Printer(
        id = "printer-1",
        tenantId = "tenant-1",
        agentId = "agent-1",
        serialNumber = "serial-1",
        name = "Printer",
        model = "A1",
        status = status,
        lastSeenAt = "2026-01-01T00:00:00Z",
        createdAt = "2026-01-01T00:00:00Z",
        nozzleTemperatures = emptyList(),
        activeNozzle = null,
        bedTemperatureCelsius = null,
        bedTargetTemperatureCelsius = null,
        chamberTemperatureCelsius = null,
        chamberLightOn = null,
        materials = null,
    )

    private fun command(
        id: String = "command-1",
        printerId: String?,
    ) = Command(
        id = id,
        tenantId = "tenant-1",
        agentId = "agent-1",
        printerId = printerId,
        kind = "printer_operation",
        status = "completed",
        payloadJson = "{}",
        error = null,
        resultJson = null,
        createdAt = "2026-01-01T00:00:00Z",
        updatedAt = "2026-01-01T00:00:00Z",
    )
}
