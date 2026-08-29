package zip.iptables.pandar.android.data.repository

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.Printer

internal data class PrinterDomainState(
    val revision: Long = 0,
    val printers: LinkedHashMap<String, Printer> = linkedMapOf(),
    val printerVersions: Map<String, Long> = emptyMap(),
    val jobs: LinkedHashMap<String, Job> = linkedMapOf(),
    val jobVersions: Map<String, Long> = emptyMap(),
    val latestCommandsByPrinter: Map<String, Command> = emptyMap(),
)

internal sealed interface PrinterStateUpdate {
    data class PrinterListLoaded(val printers: List<Printer>, val startedAtRevision: Long) : PrinterStateUpdate
    data class PrinterLoaded(val printer: Printer, val startedAtRevision: Long) : PrinterStateUpdate
    data class PrinterSnapshot(val printer: Printer) : PrinterStateUpdate
    data class JobListLoaded(val jobs: List<Job>, val startedAtRevision: Long) : PrinterStateUpdate
    data class JobProgress(val job: Job) : PrinterStateUpdate
    data class CommandResult(val command: Command) : PrinterStateUpdate
}

internal fun reducePrinterState(
    state: PrinterDomainState,
    update: PrinterStateUpdate,
): PrinterDomainState {
    val revision = state.revision + 1
    return when (update) {
        is PrinterStateUpdate.PrinterListLoaded -> {
            val printers = linkedMapOf<String, Printer>()
            val versions = state.printerVersions.toMutableMap()
            val responseIds = update.printers.mapTo(mutableSetOf()) { it.id }

            update.printers.forEach { printer ->
                val currentVersion = state.printerVersions[printer.id] ?: 0
                if (currentVersion > update.startedAtRevision) {
                    state.printers[printer.id]?.let { printers[printer.id] = it }
                } else {
                    printers[printer.id] = printer
                    versions[printer.id] = update.startedAtRevision
                }
            }
            state.printers.forEach { (id, printer) ->
                if (id !in responseIds && (state.printerVersions[id] ?: 0) > update.startedAtRevision) {
                    printers[id] = printer
                } else if (id !in responseIds) {
                    versions[id] = update.startedAtRevision
                }
            }
            state.copy(revision = revision, printers = printers, printerVersions = versions)
        }
        is PrinterStateUpdate.PrinterLoaded -> {
            val currentVersion = state.printerVersions[update.printer.id] ?: 0
            if (currentVersion > update.startedAtRevision) {
                state.copy(revision = revision)
            } else {
                state.copy(
                    revision = revision,
                    printers = LinkedHashMap(state.printers).apply { put(update.printer.id, update.printer) },
                    printerVersions = state.printerVersions +
                        (update.printer.id to update.startedAtRevision),
                )
            }
        }
        is PrinterStateUpdate.PrinterSnapshot -> state.copy(
            revision = revision,
            printers = LinkedHashMap(state.printers).apply { put(update.printer.id, update.printer) },
            printerVersions = state.printerVersions + (update.printer.id to revision),
        )
        is PrinterStateUpdate.JobListLoaded -> {
            val jobs = linkedMapOf<String, Job>()
            val versions = state.jobVersions.toMutableMap()
            val responseIds = update.jobs.mapTo(mutableSetOf()) { it.id }

            update.jobs.forEach { job ->
                val currentVersion = state.jobVersions[job.id] ?: 0
                if (currentVersion > update.startedAtRevision) {
                    state.jobs[job.id]?.let { jobs[job.id] = it }
                } else {
                    jobs[job.id] = job
                    versions[job.id] = update.startedAtRevision
                }
            }
            state.jobs.forEach { (id, job) ->
                if (id !in responseIds && (state.jobVersions[id] ?: 0) > update.startedAtRevision) {
                    jobs[id] = job
                } else if (id !in responseIds) {
                    versions[id] = update.startedAtRevision
                }
            }
            state.copy(revision = revision, jobs = jobs, jobVersions = versions)
        }
        is PrinterStateUpdate.JobProgress -> state.copy(
            revision = revision,
            jobs = LinkedHashMap(state.jobs).apply { put(update.job.id, update.job) },
            jobVersions = state.jobVersions + (update.job.id to revision),
        )
        is PrinterStateUpdate.CommandResult -> state.copy(
            revision = revision,
            latestCommandsByPrinter = update.command.printerId?.let { printerId ->
                state.latestCommandsByPrinter + (printerId to update.command)
            } ?: state.latestCommandsByPrinter,
        )
    }
}

internal class PrinterStateStore {
    private var state = PrinterDomainState()
    private val _printers = MutableStateFlow<List<Printer>>(emptyList())
    private val _jobs = MutableStateFlow<List<Job>>(emptyList())
    private val _latestCommandsByPrinter = MutableStateFlow<Map<String, Command>>(emptyMap())

    val printers: StateFlow<List<Printer>> = _printers.asStateFlow()
    val jobs: StateFlow<List<Job>> = _jobs.asStateFlow()
    val latestCommandsByPrinter: StateFlow<Map<String, Command>> =
        _latestCommandsByPrinter.asStateFlow()

    @Synchronized
    fun revision(): Long = state.revision

    @Synchronized
    fun apply(update: PrinterStateUpdate) {
        state = reducePrinterState(state, update)
        _printers.value = state.printers.values.toList()
        _jobs.value = state.jobs.values.toList()
        _latestCommandsByPrinter.value = state.latestCommandsByPrinter
    }
}
