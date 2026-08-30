package zip.iptables.pandar.android.data.remote.ws

import kotlin.coroutines.resume
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
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

    internal data class SessionEvent(val generation: Long, val event: PrinterEventDto)

    private val _events = MutableSharedFlow<SessionEvent>(extraBufferCapacity = 64)
    internal val events: SharedFlow<SessionEvent> = _events.asSharedFlow()

    private val _liveState = MutableStateFlow(LiveState.DISCONNECTED)
    val liveState: StateFlow<LiveState> = _liveState.asStateFlow()

    private var loopJob: Job? = null
    private val reconnectGeneration = MutableStateFlow(0L)
    private val sessionMonitor = Any()
    private var nextSessionGeneration = 0L
    private var activeSessionGeneration: Long? = null
    private var rejectedSession: HubSession? = null

    fun start(scope: CoroutineScope, sessions: Flow<HubSession?>) {
        if (loopJob?.isActive == true) return
        loopJob = scope.launch {
            combine(sessions, reconnectGeneration) { session, generation ->
                session to generation
            }.collectLatest { (session, _) ->
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
                val fence = activateSession()
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
            if (activeSessionGeneration == frame.generation) {
                consume(frame.event)
            }
        }
    }

    private suspend fun connectLoop(
        scope: CoroutineScope,
        session: HubSession,
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
        session: HubSession,
        fence: SessionFence,
    ): Outcome {
        val requestBuilder = Request.Builder()
            .url(session.printerEventsUrl)
            .addHeader("Authorization", "Bearer ${session.accessToken}")
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

    private fun activateSession(): SessionFence = synchronized(sessionMonitor) {
        nextSessionGeneration += 1
        activeSessionGeneration = nextSessionGeneration
        SessionFence(nextSessionGeneration)
    }

    private fun isRejectedSession(session: HubSession): Boolean = synchronized(sessionMonitor) {
        rejectedSession == session
    }

    private fun clearRejectedSession() {
        synchronized(sessionMonitor) {
            rejectedSession = null
        }
    }

    private fun settleRejectedSession(scope: CoroutineScope, session: HubSession) {
        scope.launch {
            if (!tokenRefresher()) {
                invalidateSession(session)
            }
        }
    }

    private inner class SessionFence(private val generation: Long) {
        fun isActive(): Boolean = synchronized(sessionMonitor) {
            activeSessionGeneration == generation
        }

        fun reject(session: HubSession): Boolean = synchronized(sessionMonitor) {
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
                    _events.tryEmit(SessionEvent(generation, event))
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
