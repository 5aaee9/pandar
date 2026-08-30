package zip.iptables.pandar.android.core.di

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

import okhttp3.OkHttpClient
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.auth.AuthRepository
import zip.iptables.pandar.android.data.remote.ApiModule
import zip.iptables.pandar.android.data.remote.HubSession
import zip.iptables.pandar.android.data.remote.secureHubHttpUrl
import zip.iptables.pandar.android.data.remote.PandarApi
import zip.iptables.pandar.android.data.remote.ws.PrinterEventsRepository
import zip.iptables.pandar.android.data.repository.PandarRepository
import zip.iptables.pandar.android.data.settings.SettingsRepository

class AppContainer(context: Context) {

    private val appContext = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    val settings: SettingsRepository = SettingsRepository(appContext, scope)

    val logger: Logger = AndroidLogger

    private val _apiState = MutableStateFlow<PandarApi?>(null)
    val apiState: StateFlow<PandarApi?> = _apiState.asStateFlow()

    val auth: AuthRepository = AuthRepository(settings, { apiState.value }, scope, logger)

    val okHttpClient: OkHttpClient by lazy {
        ApiModule.okHttp(
            tokenProvider = settings,
            tokenRefresher = { auth.refresh() },
            clearTokens = { scope.launch { settings.clearTokens() } },
            logger = logger,
        )
    }

    private val printerEvents: PrinterEventsRepository = PrinterEventsRepository(
        client = ApiModule.webSocketHttp(),
        tokenRefresher = { auth.refresh() },
        invalidateSession = { session -> settings.clearSessionIfCurrent(session) },
        logger = logger,
    )

    val pandar: PandarRepository = PandarRepository(
        apiProvider = { apiState.value ?: throw IllegalStateException("Hub base URL is not configured.") },
        tenantProvider = { settings.currentTenant() },
        ws = printerEvents,
        scope = scope,
    )

    init {
        // Rebuild the Retrofit API whenever the hub base URL changes.
        scope.launch {
            var lastBaseUrl: String? = null
            settings.settings.collect { snapshot ->
                val baseUrl = snapshot.hubBaseUrl
                if (baseUrl != lastBaseUrl) {
                    lastBaseUrl = baseUrl
                    rebuildApi(baseUrl)
                }
            }
        }
        printerEvents.start(
            scope,
            settings.settings
                .map { snapshot ->
                    HubSession.create(
                        snapshot.hubBaseUrl,
                        snapshot.tenantId,
                        snapshot.accessToken,
                    )
                }
                .distinctUntilChanged(),
        )
    }

    private fun rebuildApi(baseUrl: String?) {
        val httpUrl = secureHubHttpUrl(baseUrl)
        _apiState.value = httpUrl?.let { ApiModule.pandarApi(it, okHttpClient) }
    }

    /** Force the live WebSocket to reconnect (used by pull-to-refresh when the stream is down). */
    fun reconnectLiveUpdates() {
        printerEvents.reconnect()
    }
}

private object AndroidLogger : Logger {
    override fun d(t: Throwable?, msg: () -> String) {
        if (t == null) Log.d(TAG, msg()) else Log.d(TAG, msg(), t)
    }
    override fun w(t: Throwable?, msg: () -> String) {
        if (t == null) Log.w(TAG, msg()) else Log.w(TAG, msg(), t)
    }
    override fun e(t: Throwable?, msg: () -> String) {
        if (t == null) Log.e(TAG, msg()) else Log.e(TAG, msg(), t)
    }
    private const val TAG = "Pandar"
}
