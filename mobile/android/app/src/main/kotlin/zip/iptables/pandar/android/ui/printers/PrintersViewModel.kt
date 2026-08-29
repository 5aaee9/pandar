package zip.iptables.pandar.android.ui.printers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.domain.model.Agent
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

class PrintersViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(PrintersUiState())
    val state: StateFlow<PrintersUiState> = _state.asStateFlow()

    init {
        viewModelScope.launch {
            container.pandar.printers.collect { printers ->
                _state.update { it.copy(printers = printers) }
            }
        }
        viewModelScope.launch {
            container.pandar.latestCommandsByPrinter.drop(1).collect { load() }
        }
        viewModelScope.launch {
            container.pandar.liveState.collect { live -> _state.update { it.copy(liveState = live) } }
        }
        load()
    }

    fun load() {
        viewModelScope.launch {
            _state.update { it.copy(loading = true, error = null) }
            try {
                container.pandar.refreshPrinters()
                val agents = container.pandar.agents()
                _state.update { it.copy(loading = false, agents = agents) }
            } catch (t: Throwable) {
                _state.update { it.copy(loading = false, error = t.message ?: "Failed to load printers") }
            }
        }
    }

    fun refresh() {
        load()
        viewModelScope.launch {
            // Force the live WebSocket to reconnect when it is down.
            if (container.pandar.liveState.value != zip.iptables.pandar.android.data.remote.ws.LiveState.CONNECTED) {
                container.reconnectLiveUpdates()
            }
        }
    }
}
