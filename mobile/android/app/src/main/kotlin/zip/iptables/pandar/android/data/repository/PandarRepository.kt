package zip.iptables.pandar.android.data.repository

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.data.remote.PandarApi
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import zip.iptables.pandar.android.data.remote.dto.toDomain
import zip.iptables.pandar.android.data.remote.dto.toRequest
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.data.remote.ws.PrinterEventsRepository
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.domain.model.PrinterControlIntent

class PandarRepository(
    private val apiProvider: () -> PandarApi,
    private val tenantProvider: () -> String?,
    private val ws: PrinterEventsRepository,
    scope: CoroutineScope,
) {
    private val store = PrinterStateStore()

    val printers: StateFlow<List<Printer>> = store.printers
    val jobs: StateFlow<List<Job>> = store.jobs
    val latestCommandsByPrinter: StateFlow<Map<String, Command>> = store.latestCommandsByPrinter
    val liveState: StateFlow<LiveState> = ws.liveState

    private val api: PandarApi get() = apiProvider()
    private fun tenant(): String =
        tenantProvider() ?: throw IllegalStateException("Tenant id is not configured.")

    init {
        scope.launch {
            ws.events.collect { frame ->
                ws.consumeIfCurrent(frame) { event ->
                    val update = when (event) {
                        is PrinterEventDto.PrinterSnapshot ->
                            PrinterStateUpdate.PrinterSnapshot(event.printer.toDomain())
                        is PrinterEventDto.JobProgress ->
                            PrinterStateUpdate.JobProgress(event.job.toDomain())
                        is PrinterEventDto.CommandResult ->
                            PrinterStateUpdate.CommandResult(event.command.toDomain())
                    }
                    store.apply(update)
                }
            }
        }
    }

    suspend fun refreshPrinters() {
        val startedAtRevision = store.revision()
        val printers = api.listPrinters(tenant()).printers.map { it.toDomain() }
        store.apply(PrinterStateUpdate.PrinterListLoaded(printers, startedAtRevision))
    }

    suspend fun refreshPrinter(id: String) {
        val startedAtRevision = store.revision()
        val printer = api.getPrinter(tenant(), id).toDomain()
        store.apply(PrinterStateUpdate.PrinterLoaded(printer, startedAtRevision))
    }

    suspend fun refreshJobs() {
        val startedAtRevision = store.revision()
        val jobs = api.listJobs(tenant()).jobs.map { it.toDomain() }
        store.apply(PrinterStateUpdate.JobListLoaded(jobs, startedAtRevision))
    }

    suspend fun agents(): List<Agent> = api.listAgents(tenant()).agents.map { it.toDomain() }

    suspend fun control(printerId: String, intent: PrinterControlIntent): Command =
        api.control(tenant(), printerId, intent.toRequest()).toDomain()

    suspend fun retry(jobId: String): Command = api.retryDispatch(tenant(), jobId).toDomain()
    suspend fun reprint(jobId: String): Command = api.reprint(tenant(), jobId).toDomain()
}
