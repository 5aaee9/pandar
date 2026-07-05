package zip.iptables.pandar.android.ui.printers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import zip.iptables.pandar.android.data.remote.dto.toDomain
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
            container.pandar.events.collect(::applyEvent)
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
                val printers = container.pandar.printers()
                val agents = container.pandar.agents()
                _state.update { it.copy(loading = false, printers = printers, agents = agents) }
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

    private fun applyEvent(event: PrinterEventDto) {
        when (event) {
            is PrinterEventDto.PrinterSnapshot -> _state.update { state ->
                val mapped = event.printer.toDomain()
                val replaced = state.printers.map { if (it.id == mapped.id) mapped else it }
                state.copy(printers = if (replaced.any { it.id == mapped.id }) replaced else replaced + mapped)
            }
            is PrinterEventDto.CommandResult -> load()
            is PrinterEventDto.JobProgress -> Unit
        }
    }
}
