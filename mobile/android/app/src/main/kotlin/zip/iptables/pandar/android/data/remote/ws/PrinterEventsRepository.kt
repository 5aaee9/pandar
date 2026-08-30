package zip.iptables.pandar.android.data.remote.ws

import kotlin.coroutines.resume
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.HubSession
import zip.iptables.pandar.android.data.remote.HubSessionContext
import zip.iptables.pandar.android.data.remote.appJson
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import kotlin.math.min

enum class LiveState { CONNECTED, CONNECTING, DISCONNECTED }

class PrinterEventsRepository(
    private val client: OkHttpClient,
    private val tokenRefresher: suspend () -> Boolean,
    private val invalidateSession: suspend (HubSession) -> Unit,
    private val logger: Logger,
) {

    internal data class SessionEvent(
        val generation: Long,
        val sequence: Long,
        val session: HubSessionContext,
        val event: PrinterEventDto,
    )

    private val _events = MutableSharedFlow<SessionEvent>(extraBufferCapacity = 64)
    internal val events: SharedFlow<SessionEvent> = _events.asSharedFlow()
    private val resyncChannel = Channel<HubSessionContext>(Channel.CONFLATED)
    internal val resyncRequests: Flow<HubSessionContext> = resyncChannel.receiveAsFlow()
    private val droppedCommands = linkedMapOf<String, SessionEvent>()
    private val commandRecoveryChannel = Channel<Unit>(Channel.CONFLATED)
    internal val commandRecoveryRequests: Flow<Unit> = commandRecoveryChannel.receiveAsFlow()

    private val _liveState = MutableStateFlow(LiveState.DISCONNECTED)
    val liveState: StateFlow<LiveState> = _liveState.asStateFlow()

    private var loopJob: Job? = null
    private val reconnectGeneration = MutableStateFlow(0L)
    private val sessionMonitor = Any()
    private var nextSessionGeneration = 0L
    private var activeSessionGeneration: Long? = null
    private var deliverySessionContext: HubSessionContext? = null
    private var sequenceSession: HubSessionContext? = null
    private var nextFrameSequence = 0L
    private var rejectedSession: HubSessionContext? = null

    fun start(scope: CoroutineScope, sessions: Flow<HubSessionContext?>) {
        if (loopJob?.isActive == true) return
        loopJob = scope.launch {
            combine(sessions, reconnectGeneration) { session, generation ->
                session to generation
            }.collectLatest { (session, _) ->
                synchronized(sessionMonitor) {
                    deliverySessionContext = session
                }
                if (session == null) {
                    clearRejectedSession()
                    _liveState.value = LiveState.DISCONNECTED
                    return@collectLatest
                }
                if (isRejectedSession(session)) {
                    _liveState.value = LiveState.DISCONNECTED
                    return@collectLatest
                }
                clearRejectedSession()
                val fence = activateSession(session)
                try {
                    connectLoop(scope, session, fence)
                } finally {
                    fence.close()
                }
            }
        }
    }

    fun stop() {
        synchronized(sessionMonitor) {
            activeSessionGeneration = null
            deliverySessionContext = null
            droppedCommands.clear()
            _liveState.value = LiveState.DISCONNECTED
        }
        loopJob?.cancel()
        loopJob = null
    }

    fun reconnect() {
        reconnectGeneration.update { it + 1 }
    }

    internal fun consumeIfCurrent(frame: SessionEvent, consume: (PrinterEventDto) -> Unit) {
        synchronized(sessionMonitor) {
            if (deliverySessionContext == frame.session) {
                consume(frame.event)
            }
        }
    }


    internal fun drainDroppedCommands(): List<SessionEvent> = synchronized(sessionMonitor) {
        droppedCommands.values.toList().also { droppedCommands.clear() }
    }

    internal fun currentEventSequence(session: HubSessionContext): Long =
        synchronized(sessionMonitor) {
            if (sequenceSession == session) nextFrameSequence else 0
        }

    private suspend fun connectLoop(
        scope: CoroutineScope,
        session: HubSessionContext,
        fence: SessionFence,
    ) {
        val backoff = ReconnectBackoff(
            initialMs = INITIAL_BACKOFF_MS,
            maxMs = MAX_BACKOFF_MS,
        )
        while (true) {
            fence.setLiveState(LiveState.CONNECTING)
            when (openOnce(scope, session, fence)) {
                Outcome.AuthFailure -> {
                    fence.setLiveState(LiveState.DISCONNECTED)
                    return
                }
                Outcome.ClosedWithoutTraffic -> {
                    fence.setLiveState(LiveState.DISCONNECTED)
                    delay(backoff.currentMs)
                    backoff.advanceAfterFailure()
                }
                Outcome.ClosedAfterTraffic -> {
                    // The stream delivered at least one frame, so the session
                    // was healthy; retry promptly instead of compounding the
                    // cold-start backoff.
                    fence.setLiveState(LiveState.DISCONNECTED)
                    backoff.reset()
                    delay(backoff.currentMs)
                }
            }
        }
    }

    private suspend fun openOnce(
        scope: CoroutineScope,
        session: HubSessionContext,
        fence: SessionFence,
    ): Outcome {
        val requestBuilder = Request.Builder()
            .url(session.identity.printerEventsUrl)
            .addHeader("Authorization", "Bearer ${session.identity.accessToken}")
        return suspendCancellableCoroutine { cont: CancellableContinuation<Outcome> ->
            // Track whether at least one frame was received. Per spec §4.2, a socket that opens and
            // then closes immediately without any frame is treated as a probable auth failure.
            var receivedFrame = false
            fun resumeIfCurrent(outcome: Outcome) {
                if (fence.isActive() && cont.isActive) {
                    cont.resume(outcome)
                }
            }
            fun rejectAndResume(): Boolean =
                if (fence.reject(session)) {
                    settleRejectedSession(scope, session)
                    resumeIfCurrent(Outcome.AuthFailure)
                    true
                } else {
                    false
                }
            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    fence.setLiveState(LiveState.CONNECTED)
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    if (!fence.isActive()) return
                    receivedFrame = true
                    try {
                        val event = appJson.decodeFromString<PrinterEventDto>(text)
                        fence.emit(event)
                    } catch (t: Throwable) {
                        logger.w(t) { "Failed to decode printer event frame" }
                    }
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (isAuthFailure(response?.code, t)) {
                        if (rejectAndResume()) {
                            logger.w(t) { "WS auth failure (code=${response?.code})" }
                        }
                    } else {
                        if (!fence.isActive()) return
                        logger.w(t) { "WS connection failed (code=${response?.code})" }
                        resumeIfCurrent(outcomeAfterClose(receivedFrame))
                    }
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (isAuthFailure(code, null)) {
                        rejectAndResume()
                    } else if (!receivedFrame) {
                        // Upgraded then immediately closed with no frames: probable auth rejection.
                        rejectAndResume()
                    } else {
                        if (!fence.isActive()) return
                        resumeIfCurrent(outcomeAfterClose(receivedFrame))
                    }
                }
            }
            val ws = client.newWebSocket(requestBuilder.build(), listener)
            cont.invokeOnCancellation {
                fence.close()
                ws.cancel()
            }
        }
    }

    private fun activateSession(session: HubSessionContext): SessionFence = synchronized(sessionMonitor) {
        nextSessionGeneration += 1
        activeSessionGeneration = nextSessionGeneration
        if (sequenceSession != session) {
            sequenceSession = session
            nextFrameSequence = 0
        }
        SessionFence(nextSessionGeneration, session)
    }

    private fun isRejectedSession(session: HubSessionContext): Boolean = synchronized(sessionMonitor) {
        rejectedSession == session
    }

    private fun clearRejectedSession() {
        synchronized(sessionMonitor) {
            rejectedSession = null
        }
    }

    private fun settleRejectedSession(scope: CoroutineScope, session: HubSessionContext) {
        scope.launch {
            if (!tokenRefresher()) {
                invalidateSession(session.identity)
            }
        }
    }

    private inner class SessionFence(
        private val generation: Long,
        private val session: HubSessionContext,
    ) {
        fun isActive(): Boolean = synchronized(sessionMonitor) {
            activeSessionGeneration == generation
        }

        fun reject(session: HubSessionContext): Boolean = synchronized(sessionMonitor) {
            if (activeSessionGeneration == generation && rejectedSession != session) {
                rejectedSession = session
                true
            } else {
                false
            }
        }

        fun setLiveState(state: LiveState) {
            synchronized(sessionMonitor) {
                if (activeSessionGeneration == generation) {
                    _liveState.value = state
                }
            }
        }

        fun emit(event: PrinterEventDto) {
            synchronized(sessionMonitor) {
                if (activeSessionGeneration == generation) {
                    nextFrameSequence += 1
                    val frame = SessionEvent(
                        generation,
                        nextFrameSequence,
                        session,
                        event,
                    )
                    if (!_events.tryEmit(frame)) {
                        val command = event as? PrinterEventDto.CommandResult
                        command?.command?.printerId?.let { printerId ->
                            droppedCommands[printerId] = frame
                            commandRecoveryChannel.trySend(Unit)
                        }
                        resyncChannel.trySend(session)
                    }
                }
            }
        }

        fun close() {
            synchronized(sessionMonitor) {
                if (activeSessionGeneration == generation) {
                    activeSessionGeneration = null
                    _liveState.value = LiveState.DISCONNECTED
                }
            }
        }
    }

    private fun isAuthFailure(code: Int?, t: Throwable?): Boolean {
        if (code == 401 || code == 403) return true
        val message = t?.message
        return message != null && (message.contains("401") || message.contains("403"))
    }

    private fun outcomeAfterClose(receivedFrame: Boolean): Outcome {
        return if (receivedFrame) Outcome.ClosedAfterTraffic else Outcome.ClosedWithoutTraffic
    }

    private enum class Outcome { ClosedAfterTraffic, ClosedWithoutTraffic, AuthFailure }

    /** Exponential reconnect delay that only healthy sessions reset. */
    internal class ReconnectBackoff(private val initialMs: Long, private val maxMs: Long) {
        var currentMs: Long = initialMs
            private set

        fun reset() {
            currentMs = initialMs
        }

        /** Returns the delay that just elapsed and schedules the next, longer one. */
        fun advanceAfterFailure(): Long {
            val elapsed = currentMs
            currentMs = min(currentMs * 2, maxMs)
            return elapsed
        }
    }

    companion object {
        private const val INITIAL_BACKOFF_MS = 1_000L
        private const val MAX_BACKOFF_MS = 30_000L
    }
}
