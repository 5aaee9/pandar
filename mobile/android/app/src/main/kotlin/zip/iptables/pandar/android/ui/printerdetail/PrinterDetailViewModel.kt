package zip.iptables.pandar.android.ui.printerdetail

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.data.repository.PandarDataSource
import zip.iptables.pandar.android.domain.model.PandarState
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.domain.model.PrinterAxis
import zip.iptables.pandar.android.domain.model.PrinterControlIntent
import zip.iptables.pandar.android.domain.model.moveAxisIntent

data class PrinterDetailUiState(
    val loading: Boolean = true,
    val printer: Printer? = null,
    val inFlight: Boolean = false,
    val lastCommandId: String? = null,
    val lastCommandStatus: String? = null,
    val error: String? = null,
    val toast: String? = null,
)

internal data class PrinterDetailRequestState(
    val sessionGeneration: Long = 0,
    val loading: Boolean = true,
    val inFlight: Boolean = false,
    val dismissedCommandId: String? = null,
    val error: String? = null,
    val toast: String? = null,
)

internal fun printerDetailUiState(
    domain: PandarState,
    printerId: String,
    request: PrinterDetailRequestState,
): PrinterDetailUiState {
    val current = request.takeIf { it.sessionGeneration == domain.sessionGeneration }
        ?: PrinterDetailRequestState(sessionGeneration = domain.sessionGeneration)
    val command = domain.latestCommandsByPrinter[printerId]
    return PrinterDetailUiState(
        loading = current.loading,
        printer = domain.printers.find { it.id == printerId },
        inFlight = current.inFlight,
        lastCommandId = command?.id,
        lastCommandStatus = command?.status,
        error = current.error,
        toast = command
            ?.takeUnless { it.id == current.dismissedCommandId }
            ?.let { "Command ${it.id.take(8)}: ${it.status}" }
            ?: current.toast,
    )
}

class PrinterDetailViewModel(
    private val pandar: PandarDataSource,
    private val printerId: String,
) : ViewModel() {
    private val request = MutableStateFlow(
        PrinterDetailRequestState(sessionGeneration = pandar.state.value.sessionGeneration),
    )
    private var loadJob: Job? = null
    private var controlJob: Job? = null

    val state: StateFlow<PrinterDetailUiState> = combine(
        pandar.state,
        request,
    ) { domain, request -> printerDetailUiState(domain, printerId, request) }
        .stateIn(
            viewModelScope,
            SharingStarted.Eagerly,
            printerDetailUiState(pandar.state.value, printerId, request.value),
        )

    init {
        viewModelScope.launch {
            pandar.state
                .map { it.sessionGeneration to it.hasSession }
                .distinctUntilChanged()
                .collect { (generation, hasSession) ->
                    if (request.value.sessionGeneration != generation) {
                        loadJob?.cancel()
                        controlJob?.cancel()
                        request.value = PrinterDetailRequestState(sessionGeneration = generation)
                        if (hasSession) load()
                    }
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
                pandar.refreshPrinter(printerId)
                updateRequest(generation) { it.copy(loading = false) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                updateRequest(generation) {
                    it.copy(
                        loading = false,
                        error = error.message ?: "Failed to load printer",
                    )
                }
            }
        }
    }

    private fun control(intent: PrinterControlIntent) {
        val generation = pandar.state.value.sessionGeneration
        val commandId = pandar.state.value.latestCommandsByPrinter[printerId]?.id
        controlJob = viewModelScope.launch {
            updateRequest(generation) {
                it.copy(
                    inFlight = true,
                    dismissedCommandId = commandId,
                    toast = null,
                )
            }
            try {
                pandar.control(printerId, intent)
                updateRequest(generation) { it.copy(inFlight = false) }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                updateRequest(generation) {
                    it.copy(
                        inFlight = false,
                        error = error.message ?: "Control failed",
                        toast = "Control failed",
                    )
                }
            }
        }
    }

    private fun updateRequest(
        generation: Long,
        transform: (PrinterDetailRequestState) -> PrinterDetailRequestState,
    ) {
        request.update { current ->
            if (current.sessionGeneration == generation) transform(current) else current
        }
    }

    fun pause() = control(PrinterControlIntent.Pause)
    fun resume() = control(PrinterControlIntent.Resume)
    fun stop() = control(PrinterControlIntent.Stop)
    fun home() = control(PrinterControlIntent.Home())
    fun moveAxis(axis: PrinterAxis, deltaMm: Double) = control(moveAxisIntent(axis, deltaMm))
    fun toggleLight() = control(PrinterControlIntent.ToggleLight)
    fun setChamberLight(on: Boolean) = control(PrinterControlIntent.SetChamberLight(on))
    fun setHotend(temperatureCelsius: Int, wait: Boolean, extruderId: Int?) = control(
        PrinterControlIntent.SetHotendTemperature(temperatureCelsius, wait, extruderId),
    )
    fun setBed(temperatureCelsius: Int, wait: Boolean) = control(
        PrinterControlIntent.SetBedTemperature(temperatureCelsius, wait),
    )
    fun setChamber(temperatureCelsius: Int, wait: Boolean) = control(
        PrinterControlIntent.SetChamberTemperature(temperatureCelsius, wait),
    )
    fun amsReread(amsId: Int, slotId: Int) = control(
        PrinterControlIntent.AmsRereadRfid(amsId, slotId),
    )
    fun amsLoad(intent: PrinterControlIntent.AmsLoadFilament) = control(intent)
    fun amsUnload(intent: PrinterControlIntent.AmsUnloadFilament) = control(intent)
}
