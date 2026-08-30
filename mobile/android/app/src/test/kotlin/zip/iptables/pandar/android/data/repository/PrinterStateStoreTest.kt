package zip.iptables.pandar.android.data.repository

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import zip.iptables.pandar.android.data.remote.HubSession
import zip.iptables.pandar.android.data.remote.HubSessionContext
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Printer

class PrinterStateStoreTest {
    private var nextSessionEpoch = 0L

    @Test
    fun `reconnect snapshots update retained repository state without clearing it`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(session, PrinterStateUpdate.PrinterSnapshot(printer(status = "idle")))

        assertEquals("idle", store.state.value.printers.single().status)

        store.apply(session, PrinterStateUpdate.PrinterSnapshot(printer(status = "running")))

        assertEquals(1, store.state.value.printers.size)
        assertEquals("running", store.state.value.printers.single().status)
    }

    @Test
    fun `repository projects command results by printer without transport DTOs`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(session, PrinterStateUpdate.CommandResult(command(printerId = "printer-1")))
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(command(id = "tenant-command", printerId = null)),
        )

        assertEquals("command-1", store.state.value.latestCommandsByPrinter["printer-1"]?.id)
        assertFalse(store.state.value.latestCommandsByPrinter.containsKey("tenant-command"))
    }

    @Test
    fun `accepted REST command cannot overwrite a newer terminal event`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        val startedAtRevision = store.revision(session)!!
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(printerId = "printer-1", status = "succeeded"),
            ),
        )

        store.apply(
            session,
            PrinterStateUpdate.CommandAccepted(
                command(printerId = "printer-1", status = "sent"),
                startedAtRevision,
            ),
        )

        assertEquals(
            "succeeded",
            store.state.value.latestCommandsByPrinter["printer-1"]?.status,
        )
    }

    @Test
    fun `accepted new command replaces a late result for the previous command`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        val startedAtRevision = store.revision(session)!!
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "old-command", printerId = "printer-1"),
            ),
        )

        store.apply(
            session,
            PrinterStateUpdate.CommandAccepted(
                command(id = "new-command", printerId = "printer-1", status = "sent"),
                startedAtRevision,
            ),
        )

        assertEquals(
            "new-command",
            store.state.value.latestCommandsByPrinter["printer-1"]?.id,
        )
    }

    @Test
    fun `delayed acceptance cannot regress a terminal command after another becomes current`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "command-a", printerId = "printer-1", status = "succeeded"),
                eventSequence = 1,
            ),
        )
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "command-b", printerId = "printer-1", status = "acknowledged"),
                eventSequence = 2,
            ),
        )

        store.apply(
            session,
            PrinterStateUpdate.CommandAccepted(
                command(id = "command-a", printerId = "printer-1", status = "sent"),
                startedAtRevision = 0,
                observedEventSequence = 2,
            ),
        )

        assertEquals(
            "command-b",
            store.state.value.latestCommandsByPrinter["printer-1"]?.id,
        )
    }

    @Test
    fun `unrelated printer terminal churn cannot evict an outstanding command`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "command-a", printerId = "printer-1", status = "succeeded"),
                eventSequence = 1,
            ),
        )
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "command-b", printerId = "printer-1", status = "acknowledged"),
                eventSequence = 2,
            ),
        )
        repeat(300) { index ->
            store.apply(
                session,
                PrinterStateUpdate.CommandResult(
                    command(
                        id = "unrelated-$index",
                        printerId = "printer-2",
                        status = "succeeded",
                    ),
                    eventSequence = (index + 3).toLong(),
                ),
            )
        }

        store.apply(
            session,
            PrinterStateUpdate.CommandAccepted(
                command(id = "command-a", printerId = "printer-1", status = "sent"),
                startedAtRevision = 0,
                observedEventSequence = 302,
            ),
        )

        assertEquals(
            "command-b",
            store.state.value.latestCommandsByPrinter["printer-1"]?.id,
        )
    }

    @Test
    fun `session replacement atomically clears state and fences stale writes`() {
        val store = PrinterStateStore()
        val first = session("token-1")
        val second = session("token-2")
        store.replaceOwner(first)
        store.apply(first, PrinterStateUpdate.PrinterSnapshot(printer(status = "idle")))

        store.replaceOwner(second)
        assertFalse(
            store.apply(
                first,
                PrinterStateUpdate.PrinterSnapshot(printer(status = "running")),
            ),
        )

        assertEquals(emptyList<Printer>(), store.state.value.printers)
        assertEquals(emptyList<zip.iptables.pandar.android.domain.model.Job>(), store.state.value.jobs)
        assertEquals(emptyMap<String, Command>(), store.state.value.latestCommandsByPrinter)
    }

    @Test
    fun `command event applied after acceptance sample wins the linearization race`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "event-command", printerId = "printer-1"),
                eventSequence = 2,
            ),
        )

        store.apply(
            session,
            PrinterStateUpdate.CommandAccepted(
                command(id = "accepted-command", printerId = "printer-1", status = "sent"),
                startedAtRevision = 0,
                observedEventSequence = 1,
            ),
        )

        assertEquals(
            "event-command",
            store.state.value.latestCommandsByPrinter["printer-1"]?.id,
        )
    }

    @Test
    fun `recovered previous command cannot overwrite a newer accepted command`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(
            session,
            PrinterStateUpdate.CommandAccepted(
                command(id = "new-command", printerId = "printer-1", status = "sent"),
                startedAtRevision = 0,
                observedEventSequence = 2,
            ),
        )

        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "old-command", printerId = "printer-1"),
                eventSequence = 1,
            ),
        )

        assertEquals(
            "new-command",
            store.state.value.latestCommandsByPrinter["printer-1"]?.id,
        )
    }

    @Test
    fun `older recovered command event cannot overwrite a newer delivered event`() {
        val store = PrinterStateStore()
        val session = session("token-1")
        store.replaceOwner(session)
        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "new-command", printerId = "printer-1"),
                eventSequence = 2,
            ),
        )

        store.apply(
            session,
            PrinterStateUpdate.CommandResult(
                command(id = "old-command", printerId = "printer-1"),
                eventSequence = 1,
            ),
        )

        assertEquals(
            "new-command",
            store.state.value.latestCommandsByPrinter["printer-1"]?.id,
        )
    }

    @Test
    fun `returning to an equal session identity rejects the first incarnation writes`() {
        val store = PrinterStateStore()
        val identity = HubSession.create(
            "http://127.0.0.1:8080",
            "tenant-1",
            "same-token",
        )!!
        val first = HubSessionContext(identity, 1)
        val other = HubSessionContext(
            HubSession.create("http://127.0.0.1:8080", "tenant-2", "other-token")!!,
            2,
        )
        val returned = HubSessionContext(identity, 3)
        store.replaceOwner(first)
        store.replaceOwner(other)
        store.replaceOwner(returned)

        assertFalse(
            store.apply(
                first,
                PrinterStateUpdate.PrinterSnapshot(printer(status = "stale")),
            ),
        )
        assertTrue(store.state.value.printers.isEmpty())
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

    private fun session(token: String): HubSessionContext {
        nextSessionEpoch += 1
        return HubSessionContext(
            HubSession.create("http://127.0.0.1:8080", "tenant-1", token)!!,
            nextSessionEpoch,
        )
    }

    private fun command(
        id: String = "command-1",
        printerId: String?,
        status: String = "succeeded",
    ) = Command(
        id = id,
        tenantId = "tenant-1",
        agentId = "agent-1",
        printerId = printerId,
        kind = "printer_operation",
        status = status,
        payloadJson = "{}",
        error = null,
        resultJson = null,
        createdAt = "2026-01-01T00:00:00Z",
        updatedAt = "2026-01-01T00:00:00Z",
    )
}
