package zip.iptables.pandar.android.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.JobArtifact
import zip.iptables.pandar.android.domain.model.JobPrint
import zip.iptables.pandar.android.domain.model.PandarState
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.ui.jobs.JobsRequestState
import zip.iptables.pandar.android.ui.jobs.jobsUiState
import zip.iptables.pandar.android.ui.printerdetail.PrinterDetailRequestState
import zip.iptables.pandar.android.ui.printerdetail.printerDetailUiState
import zip.iptables.pandar.android.ui.printers.PrintersRequestState
import zip.iptables.pandar.android.ui.printers.printersUiState

class PandarUiStateProjectionTest {
    @Test
    fun `screen states project one atomic domain snapshot`() {
        val printer = printer()
        val job = job()
        val command = command()
        val domain = PandarState(
            sessionGeneration = 3,
            hasSession = true,
            printers = listOf(printer),
            jobs = listOf(job),
            latestCommandsByPrinter = mapOf(printer.id to command),
        )
        val agent = Agent("agent-1", "tenant-1", "Agent", "online", "created")

        val printers = printersUiState(
            domain,
            LiveState.CONNECTED,
            PrintersRequestState(
                sessionGeneration = 3,
                loading = false,
                agents = listOf(agent),
            ),
        )
        val jobs = jobsUiState(
            domain,
            JobsRequestState(
                sessionGeneration = 3,
                loading = false,
                inFlightId = job.id,
            ),
        )
        val detail = printerDetailUiState(
            domain,
            printer.id,
            PrinterDetailRequestState(sessionGeneration = 3, loading = false),
        )

        assertEquals(listOf(printer), printers.printers)
        assertEquals(listOf(agent), printers.agents)
        assertEquals(LiveState.CONNECTED, printers.liveState)
        assertEquals(listOf(job), jobs.jobs)
        assertEquals(job.id, jobs.inFlightId)
        assertEquals(printer, detail.printer)
        assertEquals(command.id, detail.lastCommandId)
    }

    @Test
    fun `projections reject transient state from a replaced session`() {
        val domain = PandarState(sessionGeneration = 2, hasSession = true)
        val agent = Agent("old-agent", "old-tenant", "Old", "online", "created")

        val printers = printersUiState(
            domain,
            LiveState.CONNECTED,
            PrintersRequestState(
                sessionGeneration = 1,
                loading = false,
                agents = listOf(agent),
                error = "old error",
            ),
        )
        val jobs = jobsUiState(
            domain,
            JobsRequestState(
                sessionGeneration = 1,
                loading = false,
                inFlightId = "old-job",
            ),
        )
        val detail = printerDetailUiState(
            domain,
            "old-printer",
            PrinterDetailRequestState(
                sessionGeneration = 1,
                loading = false,
                toast = "old toast",
            ),
        )

        assertTrue(printers.loading)
        assertTrue(printers.agents.isEmpty())
        assertEquals(null, printers.error)
        assertTrue(jobs.loading)
        assertEquals(null, jobs.inFlightId)
        assertTrue(detail.loading)
        assertEquals(null, detail.toast)
    }

    private fun printer() = Printer(
        id = "printer-1",
        tenantId = "tenant-1",
        agentId = "agent-1",
        serialNumber = "serial-1",
        name = "Printer",
        model = "A1",
        status = "idle",
        lastSeenAt = "seen",
        createdAt = "created",
        nozzleTemperatures = emptyList(),
        activeNozzle = null,
        bedTemperatureCelsius = null,
        bedTargetTemperatureCelsius = null,
        chamberTemperatureCelsius = null,
        chamberLightOn = null,
        materials = null,
    )

    private fun job() = Job(
        id = "job-1",
        printerId = "printer-1",
        agentId = "agent-1",
        artifactId = "artifact-1",
        commandId = "command-1",
        status = "running",
        error = null,
        createdAt = "created",
        updatedAt = "updated",
        print = JobPrint(
            status = "running",
            progressPercent = 10,
            remainingTimeMinutes = null,
            currentLayer = null,
            totalLayers = null,
            activeFile = null,
            error = null,
            startedAt = null,
            finishedAt = null,
            updatedAt = null,
        ),
        artifact = JobArtifact(
            id = "artifact-1",
            filename = "part.3mf",
            contentType = "model/3mf",
            sizeBytes = 1,
            createdAt = "created",
        ),
    )

    private fun command() = Command(
        id = "command-1",
        tenantId = "tenant-1",
        agentId = "agent-1",
        printerId = "printer-1",
        kind = "printer_operation",
        status = "completed",
        payloadJson = "{}",
        error = null,
        resultJson = null,
        createdAt = "created",
        updatedAt = "updated",
    )
}
