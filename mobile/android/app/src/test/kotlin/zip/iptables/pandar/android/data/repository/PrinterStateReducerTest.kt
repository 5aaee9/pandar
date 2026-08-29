package zip.iptables.pandar.android.data.repository

import org.junit.Assert.assertEquals
import org.junit.Test
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.JobArtifact
import zip.iptables.pandar.android.domain.model.JobPrint
import zip.iptables.pandar.android.domain.model.Printer

class PrinterStateReducerTest {
    @Test
    fun `REST printer snapshot is followed by live snapshots through one reducer`() {
        val initial = reducePrinterState(
            PrinterDomainState(),
            PrinterStateUpdate.PrinterListLoaded(listOf(printer(status = "idle")), 0),
        )

        val live = reducePrinterState(
            initial,
            PrinterStateUpdate.PrinterSnapshot(printer(status = "running")),
        )

        assertEquals("running", live.printers.getValue("printer-1").status)
    }

    @Test
    fun `progress and terminal job transitions replace the same job`() {
        val initial = reducePrinterState(
            PrinterDomainState(),
            PrinterStateUpdate.JobListLoaded(listOf(job(status = "dispatched", progress = 0)), 0),
        )
        val running = reducePrinterState(
            initial,
            PrinterStateUpdate.JobProgress(job(status = "running", progress = 45)),
        )
        val completed = reducePrinterState(
            running,
            PrinterStateUpdate.JobProgress(job(status = "completed", progress = 100)),
        )

        assertEquals(1, completed.jobs.size)
        assertEquals("completed", completed.jobs.getValue("job-1").print.status)
        assertEquals(100, completed.jobs.getValue("job-1").print.progressPercent)
    }

    @Test
    fun `REST response started before live progress cannot regress terminal state`() {
        val initial = reducePrinterState(
            PrinterDomainState(),
            PrinterStateUpdate.JobListLoaded(listOf(job(status = "running", progress = 20)), 0),
        )
        val requestRevision = initial.revision
        val completed = reducePrinterState(
            initial,
            PrinterStateUpdate.JobProgress(job(status = "completed", progress = 100)),
        )

        val lateResponse = reducePrinterState(
            completed,
            PrinterStateUpdate.JobListLoaded(
                listOf(job(status = "running", progress = 20)),
                requestRevision,
            ),
        )

        assertEquals("completed", lateResponse.jobs.getValue("job-1").print.status)
    }

    @Test
    fun `newer overlapping printer refresh wins after older refresh completes first`() {
        val firstStartedAt = 0L
        val interveningEvent = reducePrinterState(
            PrinterDomainState(),
            PrinterStateUpdate.JobProgress(job(status = "running", progress = 10)),
        )
        val secondStartedAt = interveningEvent.revision
        val firstCompleted = reducePrinterState(
            interveningEvent,
            PrinterStateUpdate.PrinterListLoaded(
                listOf(printer(status = "response-a")),
                firstStartedAt,
            ),
        )

        val secondCompleted = reducePrinterState(
            firstCompleted,
            PrinterStateUpdate.PrinterListLoaded(
                listOf(printer(status = "response-b")),
                secondStartedAt,
            ),
        )

        assertEquals("response-b", secondCompleted.printers.getValue("printer-1").status)
    }

    @Test
    fun `late initial printer response keeps newer event values and response ordering`() {
        val requestRevision = 0L
        val eventState = reducePrinterState(
            PrinterDomainState(),
            PrinterStateUpdate.PrinterSnapshot(printer(id = "printer-2", status = "running")),
        )

        val reduced = reducePrinterState(
            eventState,
            PrinterStateUpdate.PrinterListLoaded(
                listOf(
                    printer(id = "printer-1", status = "idle"),
                    printer(id = "printer-2", status = "offline"),
                ),
                requestRevision,
            ),
        )

        assertEquals(listOf("printer-1", "printer-2"), reduced.printers.keys.toList())
        assertEquals("running", reduced.printers.getValue("printer-2").status)
    }

    private fun printer(
        id: String = "printer-1",
        status: String,
    ) = Printer(
        id = id,
        tenantId = "tenant-1",
        agentId = "agent-1",
        serialNumber = "serial-$id",
        name = id,
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

    private fun job(status: String, progress: Int) = Job(
        id = "job-1",
        printerId = "printer-1",
        agentId = "agent-1",
        artifactId = "artifact-1",
        commandId = "command-1",
        status = "dispatched",
        error = null,
        createdAt = "2026-01-01T00:00:00Z",
        updatedAt = "2026-01-01T00:00:00Z",
        print = JobPrint(
            status = status,
            progressPercent = progress,
            remainingTimeMinutes = null,
            currentLayer = null,
            totalLayers = null,
            activeFile = null,
            error = null,
            startedAt = null,
            finishedAt = null,
            updatedAt = "2026-01-01T00:00:00Z",
        ),
        artifact = JobArtifact(
            id = "artifact-1",
            filename = "part.3mf",
            contentType = "model/3mf",
            sizeBytes = 1,
            createdAt = "2026-01-01T00:00:00Z",
        ),
    )
}
