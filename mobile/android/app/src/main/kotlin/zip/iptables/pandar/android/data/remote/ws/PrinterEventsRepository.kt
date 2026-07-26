package zip.iptables.pandar.android.data.remote.ws

import kotlin.coroutines.resume
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import zip.iptables.pandar.android.data.remote.secureHubHttpUrl
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.appJson
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import kotlin.math.min

enum class LiveState { CONNECTED, CONNECTING, DISCONNECTED }

class PrinterEventsRepository(
    private val client: OkHttpClient,
    private val hubBaseUrl: () -> String?,
    private val tenantId: () -> String?,
    private val tokenProvider: () -> String?,
    private val tokenRefresher: suspend () -> Boolean,
    private val logger: Logger,
) {

    private val _events = MutableSharedFlow<PrinterEventDto>(extraBufferCapacity = 64)
    val events: SharedFlow<PrinterEventDto> = _events.asSharedFlow()

    private val _liveState = MutableStateFlow(LiveState.DISCONNECTED)
    val liveState: StateFlow<LiveState> = _liveState.asStateFlow()

    private val _needsReauth = MutableStateFlow(false)
    val needsReauth: StateFlow<Boolean> = _needsReauth.asStateFlow()

    private var loopJob: Job? = null

    fun start(scope: CoroutineScope) {
        if (loopJob?.isActive == true) return
        loopJob = scope.launch { connectLoop() }
    }

    fun stop() {
        loopJob?.cancel()
        loopJob = null
        _liveState.value = LiveState.DISCONNECTED
    }

    private suspend fun connectLoop() {
        var backoffMs = INITIAL_BACKOFF_MS
        var refreshedForFailure = false
        while (true) {
            val url = buildWsUrl()
            if (url == null) {
                _liveState.value = LiveState.DISCONNECTED
                delay(RETRY_INTERVAL_MS)
                continue
            }
            _liveState.value = LiveState.CONNECTING
            when (val outcome = openOnce(url)) {
                Outcome.AuthFailure -> {
                    if (!refreshedForFailure && tokenRefresher()) {
                        refreshedForFailure = true
                        backoffMs = INITIAL_BACKOFF_MS
                        continue // reconnect immediately with refreshed token
                    }
                    _needsReauth.value = true
                    _liveState.value = LiveState.DISCONNECTED
                    delay(backoffMs)
                    backoffMs = nextBackoff(backoffMs)
                }
                Outcome.Closed -> {
                    refreshedForFailure = false
                    _liveState.value = LiveState.DISCONNECTED
                    delay(backoffMs)
                    backoffMs = nextBackoff(backoffMs)
                }
                Outcome.Connected -> {
                    refreshedForFailure = false
                    backoffMs = INITIAL_BACKOFF_MS
                }
            }
        }
    }

    private suspend fun openOnce(url: String): Outcome {
        val requestBuilder = Request.Builder().url(url)
        tokenProvider()?.takeIf { it.isNotEmpty() }?.let { token ->
            requestBuilder.addHeader("Authorization", "Bearer $token")
        }
        return suspendCancellableCoroutine { cont: CancellableContinuation<Outcome> ->
            // Track whether at least one frame was received. Per spec §4.2, a socket that opens and
            // then closes immediately without any frame is treated as a probable auth failure.
            var receivedFrame = false
            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    _liveState.value = LiveState.CONNECTED
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    receivedFrame = true
                    try {
                        val event = appJson.decodeFromString<PrinterEventDto>(text)
                        _events.tryEmit(event)
                    } catch (t: Throwable) {
                        logger.w(t) { "Failed to decode printer event frame" }
                    }
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (isAuthFailure(response?.code, t)) {
                        logger.w(t) { "WS auth failure (code=${response?.code})" }
                        cont.resume(Outcome.AuthFailure)
                    } else if (!receivedFrame && response != null && (response.code == 401 || response.code == 403)) {
                        // Confirmed auth-rejected upgrade before any frame.
                        logger.w(t) { "WS rejected before any frame (code=${response.code})" }
                        cont.resume(Outcome.AuthFailure)
                    } else {
                        logger.w(t) { "WS connection failed (code=${response?.code})" }
                        cont.resume(Outcome.Closed)
                    }
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (isAuthFailure(code, null)) {
                        cont.resume(Outcome.AuthFailure)
                    } else if (!receivedFrame) {
                        // Upgraded then immediately closed with no frames: probable auth rejection.
                        cont.resume(Outcome.AuthFailure)
                    } else {
                        cont.resume(Outcome.Closed)
                    }
                }
            }
            val ws = client.newWebSocket(requestBuilder.build(), listener)
            cont.invokeOnCancellation { ws.cancel() }
        }
    }

    private fun buildWsUrl(): String? {
        val base = secureHubHttpUrl(hubBaseUrl()) ?: return null
        val tenant = tenantId()?.trim()?.takeIf { it.isNotEmpty() } ?: return null
        val wsBase = base.newBuilder()
            .scheme(if (base.isHttps) "wss" else "ws")
            .build()
            .toString()
            .trimEnd('/')
        return "$wsBase/api/v1/tenants/$tenant/printer-events"
    }

    private fun isAuthFailure(code: Int?, t: Throwable?): Boolean {
        if (code == 401 || code == 403) return true
        val message = t?.message
        return message != null && (message.contains("401") || message.contains("403"))
    }

    private fun nextBackoff(current: Long): Long = min(current * 2, MAX_BACKOFF_MS)

    private enum class Outcome { Connected, Closed, AuthFailure }

    companion object {
        private const val INITIAL_BACKOFF_MS = 1_000L
        private const val MAX_BACKOFF_MS = 30_000L
        private const val RETRY_INTERVAL_MS = 5_000L
    }
}
