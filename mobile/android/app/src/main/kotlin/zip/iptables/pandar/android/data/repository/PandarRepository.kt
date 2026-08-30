package zip.iptables.pandar.android.data.repository

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.HubApiSession
import zip.iptables.pandar.android.data.remote.HubSessionContext
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import zip.iptables.pandar.android.data.remote.dto.toDomain
import zip.iptables.pandar.android.data.remote.dto.toRequest
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.data.remote.ws.PrinterEventsRepository
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.PandarState
import zip.iptables.pandar.android.domain.model.PrinterControlIntent

interface PandarDataSource {
    val state: StateFlow<PandarState>
    val liveState: StateFlow<LiveState>

    suspend fun refreshPrinters()
    suspend fun refreshPrinter(id: String)
    suspend fun refreshJobs()
    suspend fun agents(): List<Agent>
    suspend fun control(printerId: String, intent: PrinterControlIntent): Command
    suspend fun retry(jobId: String): Command
    suspend fun reprint(jobId: String): Command
}

class PandarRepository(
    private val sessions: StateFlow<HubSessionContext?>,
    private val apiSession: (HubSessionContext) -> HubApiSession,
    private val ws: PrinterEventsRepository,
    scope: CoroutineScope,
    private val logger: Logger,
) : PandarDataSource {
    private val store = PrinterStateStore()
    private val _readySessions = MutableStateFlow<HubSessionContext?>(null)
    @Volatile
    private var activeSession: HubApiSession? = null

    internal val readySessions: StateFlow<HubSessionContext?> =
        _readySessions.asStateFlow()
    override val state: StateFlow<PandarState> = store.state
    override val liveState: StateFlow<LiveState> = ws.liveState

    init {
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            ws.events.collect { frame ->
                ws.consumeIfCurrent(frame) { event ->
                    store.apply(frame.session, event.toStateUpdate(frame.sequence))
                }
            }
        }
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            ws.commandRecoveryRequests.collect {
                ws.drainDroppedCommands().forEach { frame ->
                    store.apply(
                        frame.session,
                        frame.event.toStateUpdate(frame.sequence),
                    )
                }
            }
        }
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            ws.resyncRequests.collectLatest { identity ->
                val session = activeSession?.takeIf { it.context == identity }
                    ?: return@collectLatest
                resync(session)
            }
        }
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            sessions.collectLatest { identity ->
                _readySessions.value = null
                val session = try {
                    identity?.let(apiSession)
                } catch (error: Throwable) {
                    activeSession = null
                    store.replaceOwner(null)
                    logger.e(error) { "Failed to create Android Hub API session" }
                    return@collectLatest
                }
                activeSession = session
                store.replaceOwner(identity)
                _readySessions.value = identity
                if (session != null) {
                    resync(session)
                }
            }
        }
    }

    override suspend fun refreshPrinters() {
        refreshPrinters(requireSession())
    }

    override suspend fun refreshPrinter(id: String) {
        val session = requireSession()
        val startedAtRevision = beginRead(session)
        val printer = session.api.getPrinter(session.identity.tenantId, id).toDomain()
        applyCurrent(session, PrinterStateUpdate.PrinterLoaded(printer, startedAtRevision))
    }

    override suspend fun refreshJobs() {
        refreshJobs(requireSession())
    }

    override suspend fun agents(): List<Agent> {
        val session = requireSession()
        val agents = session.api.listAgents(session.identity.tenantId).agents.map { it.toDomain() }
        ensureCurrent(session)
        return agents
    }

    override suspend fun control(
        printerId: String,
        intent: PrinterControlIntent,
    ): Command {
        val session = requireSession()
        val startedAtRevision = beginRead(session)
        val command = session.api.control(
            session.identity.tenantId,
            printerId,
            intent.toRequest(),
        ).toDomain()
        ensureCurrent(session)
        applyCurrent(
            session,
            PrinterStateUpdate.CommandAccepted(
                command,
                startedAtRevision,
                ws.currentEventSequence(session.context),
            ),
        )
        return command
    }

    override suspend fun retry(jobId: String): Command {
        val session = requireSession()
        val startedAtRevision = beginRead(session)
        val command = session.api.retryDispatch(session.identity.tenantId, jobId).toDomain()
        ensureCurrent(session)
        applyCurrent(
            session,
            PrinterStateUpdate.CommandAccepted(
                command,
                startedAtRevision,
                ws.currentEventSequence(session.context),
            ),
        )
        return command
    }

    override suspend fun reprint(jobId: String): Command {
        val session = requireSession()
        val startedAtRevision = beginRead(session)
        val command = session.api.reprint(session.identity.tenantId, jobId).toDomain()
        ensureCurrent(session)
        applyCurrent(
            session,
            PrinterStateUpdate.CommandAccepted(
                command,
                startedAtRevision,
                ws.currentEventSequence(session.context),
            ),
        )
        return command
    }

    private suspend fun resync(session: HubApiSession) {
        try {
            refreshPrinters(session)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            logger.w(error) { "Failed to resynchronize Android printers" }
        }
        try {
            refreshJobs(session)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            logger.w(error) { "Failed to resynchronize Android jobs" }
        }
    }

    private suspend fun refreshPrinters(session: HubApiSession) {
        val startedAtRevision = beginRead(session)
        val printers = session.api.listPrinters(session.identity.tenantId).printers.map { it.toDomain() }
        applyCurrent(
            session,
            PrinterStateUpdate.PrinterListLoaded(printers, startedAtRevision),
        )
    }

    private suspend fun refreshJobs(session: HubApiSession) {
        val startedAtRevision = beginRead(session)
        val jobs = session.api.listJobs(session.identity.tenantId).jobs.map { it.toDomain() }
        applyCurrent(session, PrinterStateUpdate.JobListLoaded(jobs, startedAtRevision))
    }

    private fun beginRead(session: HubApiSession): Long =
        store.revision(session.context)
            ?: throw CancellationException("Hub session changed.")

    private fun requireSession(): HubApiSession {
        val session = requireNotNull(activeSession) { "Hub session is not configured." }
        if (!store.owns(session.context)) {
            throw CancellationException("Hub session changed.")
        }
        return session
    }

    private fun ensureCurrent(session: HubApiSession) {
        if (activeSession?.context != session.context || !store.owns(session.context)) {
            throw CancellationException("Hub session changed.")
        }
    }

    private fun applyCurrent(session: HubApiSession, update: PrinterStateUpdate) {
        if (!store.apply(session.context, update)) {
            throw CancellationException("Hub session changed.")
        }
    }
}

private fun PrinterEventDto.toStateUpdate(eventSequence: Long): PrinterStateUpdate = when (this) {
    is PrinterEventDto.PrinterSnapshot ->
        PrinterStateUpdate.PrinterSnapshot(printer.toDomain())
    is PrinterEventDto.JobProgress ->
        PrinterStateUpdate.JobProgress(job.toDomain())
    is PrinterEventDto.CommandResult ->
        PrinterStateUpdate.CommandResult(command.toDomain(), eventSequence)
}
