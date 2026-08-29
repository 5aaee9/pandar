package zip.iptables.pandar.android.data.repository

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.data.remote.PandarApi
import zip.iptables.pandar.android.data.remote.dto.AmsLoadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.AmsRereadRfidRequest
import zip.iptables.pandar.android.data.remote.dto.AmsUnloadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.MoveAxesRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import zip.iptables.pandar.android.data.remote.dto.SetBedTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberLightRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetHotendTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.toDomain
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.data.remote.ws.PrinterEventsRepository
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.Printer

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
    val needsReauth: StateFlow<Boolean> = ws.needsReauth

    private val api: PandarApi get() = apiProvider()
    private fun tenant(): String =
        tenantProvider() ?: throw IllegalStateException("Tenant id is not configured.")

    init {
        scope.launch {
            ws.events.collect { event ->
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

    suspend fun pause(printerId: String): Command = api.pause(tenant(), printerId).toDomain()
    suspend fun resume(printerId: String): Command = api.resume(tenant(), printerId).toDomain()
    suspend fun stop(printerId: String): Command = api.stop(tenant(), printerId).toDomain()
    suspend fun home(printerId: String): Command = api.home(tenant(), printerId).toDomain()
    suspend fun moveAxes(printerId: String, body: MoveAxesRequest): Command =
        api.moveAxes(tenant(), printerId, body).toDomain()
    suspend fun toggleLight(printerId: String): Command = api.toggleLight(tenant(), printerId).toDomain()
    suspend fun setChamberLight(printerId: String, on: Boolean): Command =
        api.setChamberLight(tenant(), printerId, SetChamberLightRequest(lightOn = on)).toDomain()
    suspend fun setHotendTemperature(printerId: String, body: SetHotendTemperatureRequest): Command =
        api.setHotendTemperature(tenant(), printerId, body).toDomain()
    suspend fun setBedTemperature(printerId: String, body: SetBedTemperatureRequest): Command =
        api.setBedTemperature(tenant(), printerId, body).toDomain()
    suspend fun setChamberTemperature(printerId: String, body: SetChamberTemperatureRequest): Command =
        api.setChamberTemperature(tenant(), printerId, body).toDomain()
    suspend fun amsRereadRfid(printerId: String, body: AmsRereadRfidRequest): Command =
        api.amsRereadRfid(tenant(), printerId, body).toDomain()
    suspend fun amsLoadFilament(printerId: String, body: AmsLoadFilamentRequest): Command =
        api.amsLoadFilament(tenant(), printerId, body).toDomain()
    suspend fun amsUnloadFilament(printerId: String, body: AmsUnloadFilamentRequest): Command =
        api.amsUnloadFilament(tenant(), printerId, body).toDomain()

    suspend fun retry(jobId: String): Command = api.retryDispatch(tenant(), jobId).toDomain()
    suspend fun reprint(jobId: String): Command = api.reprint(tenant(), jobId).toDomain()
}
