package zip.iptables.pandar.android.data.repository

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import zip.iptables.pandar.android.data.remote.HubSessionContext
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.PandarState
import zip.iptables.pandar.android.domain.model.Printer

internal data class PrinterDomainState(
    val revision: Long = 0,
    val printers: LinkedHashMap<String, Printer> = linkedMapOf(),
    val printerVersions: Map<String, Long> = emptyMap(),
    val jobs: LinkedHashMap<String, Job> = linkedMapOf(),
    val jobVersions: Map<String, Long> = emptyMap(),
    val latestCommandsByPrinter: Map<String, Command> = emptyMap(),
    val commandVersionsByPrinter: Map<String, Long> = emptyMap(),
    val commandEventSequencesByPrinter: Map<String, Long> = emptyMap(),
    val commandAcceptedEventSequencesByPrinter: Map<String, Long> = emptyMap(),
    val terminalCommandIdsByPrinter: Map<String, Set<String>> = emptyMap(),
)

internal sealed interface PrinterStateUpdate {
    data class PrinterListLoaded(val printers: List<Printer>, val startedAtRevision: Long) : PrinterStateUpdate
    data class PrinterLoaded(val printer: Printer, val startedAtRevision: Long) : PrinterStateUpdate
    data class PrinterSnapshot(val printer: Printer) : PrinterStateUpdate
    data class JobListLoaded(val jobs: List<Job>, val startedAtRevision: Long) : PrinterStateUpdate
    data class JobProgress(val job: Job) : PrinterStateUpdate
    data class CommandAccepted(
        val command: Command,
        val startedAtRevision: Long,
        val observedEventSequence: Long = 0,
    ) : PrinterStateUpdate
    data class CommandResult(
        val command: Command,
        val eventSequence: Long = 0,
    ) : PrinterStateUpdate
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
        is PrinterStateUpdate.CommandAccepted -> {
            val printerId = update.command.printerId
            val terminalCommandIds = forgetTerminalCommand(
                state.terminalCommandIdsByPrinter,
                update.command,
            )
            if (
                printerId == null ||
                update.command.id in
                state.terminalCommandIdsByPrinter[printerId].orEmpty() ||
                (state.commandEventSequencesByPrinter[printerId] ?: 0) >
                update.observedEventSequence ||
                (
                    state.latestCommandsByPrinter[printerId]?.id == update.command.id &&
                        (state.commandVersionsByPrinter[printerId] ?: 0) > update.startedAtRevision
                    )
            ) {
                state.copy(
                    revision = revision,
                    terminalCommandIdsByPrinter = terminalCommandIds,
                )
            } else {
                state.copy(
                    revision = revision,
                    latestCommandsByPrinter = state.latestCommandsByPrinter +
                        (printerId to update.command),
                    commandVersionsByPrinter = state.commandVersionsByPrinter +
                        (printerId to revision),
                    commandAcceptedEventSequencesByPrinter =
                        state.commandAcceptedEventSequencesByPrinter +
                        (printerId to update.observedEventSequence),
                    terminalCommandIdsByPrinter = terminalCommandIds,
                )
            }
        }
        is PrinterStateUpdate.CommandResult -> {
            val printerId = update.command.printerId
            val currentCommand = printerId?.let(state.latestCommandsByPrinter::get)
            val terminalCommandIds = rememberTerminalCommand(
                state.terminalCommandIdsByPrinter,
                update.command,
            )
            if (
                printerId == null ||
                (
                    update.eventSequence > 0 &&
                        (state.commandEventSequencesByPrinter[printerId] ?: 0) >=
                        update.eventSequence
                    ) ||
                (
                    update.eventSequence > 0 &&
                        currentCommand?.id != update.command.id &&
                        update.eventSequence <=
                        (state.commandAcceptedEventSequencesByPrinter[printerId] ?: 0)
                    )
            ) {
                state.copy(
                    revision = revision,
                    terminalCommandIdsByPrinter = terminalCommandIds,
                )
            } else {
                state.copy(
                    revision = revision,
                    latestCommandsByPrinter = state.latestCommandsByPrinter +
                        (printerId to update.command),
                    commandVersionsByPrinter = state.commandVersionsByPrinter +
                        (printerId to revision),
                    commandEventSequencesByPrinter = if (update.eventSequence > 0) {
                        state.commandEventSequencesByPrinter +
                            (printerId to update.eventSequence)
                    } else {
                        state.commandEventSequencesByPrinter
                    },
                    terminalCommandIdsByPrinter = terminalCommandIds,
                )
            }
        }
    }
}

private fun rememberTerminalCommand(
    current: Map<String, Set<String>>,
    command: Command,
): Map<String, Set<String>> {
    if (command.status !in TERMINAL_COMMAND_STATUSES) return current
    val printerId = command.printerId ?: return current
    val terminal = LinkedHashSet(current[printerId].orEmpty())
    terminal.remove(command.id)
    terminal.add(command.id)
    while (terminal.size > MAX_RECENT_TERMINAL_COMMANDS_PER_PRINTER) {
        terminal.remove(terminal.first())
    }
    return current + (printerId to terminal)
}

private fun forgetTerminalCommand(
    current: Map<String, Set<String>>,
    command: Command,
): Map<String, Set<String>> {
    val printerId = command.printerId ?: return current
    val terminal = current[printerId]?.toMutableSet() ?: return current
    if (!terminal.remove(command.id)) return current
    return if (terminal.isEmpty()) current - printerId else current + (printerId to terminal)
}

private val TERMINAL_COMMAND_STATUSES = setOf("succeeded", "failed", "cancelled")
private const val MAX_RECENT_TERMINAL_COMMANDS_PER_PRINTER = 256

internal class PrinterStateStore {
    private var owner: HubSessionContext? = null
    private var ownerGeneration = 0L
    private var reducerState = PrinterDomainState()
    private val _state = MutableStateFlow(PandarState())

    val state: StateFlow<PandarState> = _state.asStateFlow()

    @Synchronized
    fun replaceOwner(nextOwner: HubSessionContext?) {
        if (owner == nextOwner) return
        owner = nextOwner
        ownerGeneration += 1
        reducerState = PrinterDomainState()
        _state.value = PandarState(
            sessionGeneration = ownerGeneration,
            hasSession = nextOwner != null,
        )
    }

    @Synchronized
    fun owns(expectedOwner: HubSessionContext): Boolean = owner == expectedOwner

    @Synchronized
    fun revision(expectedOwner: HubSessionContext): Long? =
        reducerState.revision.takeIf { owner == expectedOwner }

    @Synchronized
    fun apply(expectedOwner: HubSessionContext, update: PrinterStateUpdate): Boolean {
        if (owner != expectedOwner) return false
        reducerState = reducePrinterState(reducerState, update)
        _state.value = PandarState(
            sessionGeneration = ownerGeneration,
            hasSession = true,
            printers = reducerState.printers.values.toList(),
            jobs = reducerState.jobs.values.toList(),
            latestCommandsByPrinter = reducerState.latestCommandsByPrinter,
        )
        return true
    }
}
