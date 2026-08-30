package zip.iptables.pandar.android.ui.printers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.data.repository.PandarDataSource
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.PandarState
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.domain.model.Severity
import zip.iptables.pandar.android.domain.status.statusMeta

data class PrintersUiState(
    val loading: Boolean = true,
    val printers: List<Printer> = emptyList(),
    val agents: List<Agent> = emptyList(),
    val liveState: LiveState = LiveState.DISCONNECTED,
    val error: String? = null,
) {
    val onlinePrinters: Int
        get() = printers.count { statusMeta(it.status).severity != Severity.CRITICAL }
    val connectedAgents: Int
        get() = agents.count { statusMeta(it.status).severity != Severity.CRITICAL }
}

internal data class PrintersRequestState(
    val sessionGeneration: Long = 0,
    val loading: Boolean = true,
    val agents: List<Agent> = emptyList(),
    val error: String? = null,
)

internal fun printersUiState(
    domain: PandarState,
    liveState: LiveState,
    request: PrintersRequestState,
): PrintersUiState {
    val current = request.takeIf { it.sessionGeneration == domain.sessionGeneration }
        ?: PrintersRequestState(sessionGeneration = domain.sessionGeneration)
    return PrintersUiState(
        loading = current.loading,
        printers = domain.printers,
        agents = current.agents,
        liveState = liveState,
        error = current.error,
    )
}

class PrintersViewModel(
    private val pandar: PandarDataSource,
    private val reconnectLiveUpdates: () -> Unit,
) : ViewModel() {
    private val request = MutableStateFlow(
        PrintersRequestState(sessionGeneration = pandar.state.value.sessionGeneration),
    )
    private var loadJob: Job? = null

    val state: StateFlow<PrintersUiState> = combine(
        pandar.state,
        pandar.liveState,
        request,
        ::printersUiState,
    ).stateIn(
        viewModelScope,
        SharingStarted.Eagerly,
        printersUiState(pandar.state.value, pandar.liveState.value, request.value),
    )

    init {
        viewModelScope.launch {
            pandar.state
                .map { it.sessionGeneration to it.hasSession }
                .distinctUntilChanged()
                .collect { (generation, hasSession) ->
                    if (request.value.sessionGeneration != generation) {
                        loadJob?.cancel()
                        request.value = PrintersRequestState(sessionGeneration = generation)
                        if (hasSession) load()
                    }
                }
        }
        viewModelScope.launch {
            pandar.state
                .map { it.latestCommandsByPrinter }
                .distinctUntilChanged()
                .drop(1)
                .collect { commands ->
                    if (commands.isNotEmpty()) load()
                }
        }
        if (pandar.state.value.hasSession) load()
    }

    fun load() {
        loadJob?.cancel()
        val generation = pandar.state.value.sessionGeneration
        loadJob = viewModelScope.launch {
            updateRequest(generation) { it.copy(loading = true, error = null) }
            try {
                pandar.refreshPrinters()
                val agents = pandar.agents()
                updateRequest(generation) {
                    it.copy(loading = false, agents = agents)
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                updateRequest(generation) {
                    it.copy(
                        loading = false,
                        error = error.message ?: "Failed to load printers",
                    )
                }
            }
        }
    }

    fun refresh() {
        load()
        if (pandar.liveState.value != LiveState.CONNECTED) {
            reconnectLiveUpdates()
        }
    }

    private fun updateRequest(
        generation: Long,
        transform: (PrintersRequestState) -> PrintersRequestState,
    ) {
        request.update { current ->
            if (current.sessionGeneration == generation) transform(current) else current
        }
    }
}
