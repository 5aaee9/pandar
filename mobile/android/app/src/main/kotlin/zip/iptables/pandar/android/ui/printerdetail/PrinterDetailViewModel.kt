package zip.iptables.pandar.android.ui.printerdetail

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.domain.model.Command
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

class PrinterDetailViewModel(
    private val container: AppContainer,
    private val printerId: String,
) : ViewModel() {

    private val _state = MutableStateFlow(PrinterDetailUiState())
    val state: StateFlow<PrinterDetailUiState> = _state.asStateFlow()

    init {
        viewModelScope.launch {
            container.pandar.printers.collect { printers ->
                printers.find { it.id == printerId }?.let { printer ->
                    _state.update { it.copy(printer = printer) }
                }
            }
        }
        viewModelScope.launch {
            container.pandar.latestCommandsByPrinter
                .map { it[printerId] }
                .drop(1)
                .filterNotNull()
                .collect { cmd ->
                    _state.update {
                        it.copy(
                            lastCommandId = cmd.id,
                            lastCommandStatus = cmd.status,
                            toast = "Command ${cmd.id.take(8)}: ${cmd.status}",
                        )
                    }
                }
        }
        load()
    }

    fun load() {
        viewModelScope.launch {
            _state.update { it.copy(loading = true, error = null) }
            try {
                container.pandar.refreshPrinter(printerId)
                _state.update { it.copy(loading = false) }
            } catch (t: Throwable) {
                _state.update { it.copy(loading = false, error = t.message ?: "Failed to load printer") }
            }
        }
    }

    private fun sendControl(action: suspend () -> Command) {
        viewModelScope.launch {
            _state.update { it.copy(inFlight = true, toast = null) }
            try {
                val cmd = action()
                _state.update {
                    it.copy(
                        inFlight = false,
                        lastCommandId = cmd.id,
                        lastCommandStatus = cmd.status,
                        toast = "Command ${cmd.id.take(8)}: ${cmd.status}",
                    )
                }
            } catch (t: Throwable) {
                _state.update { it.copy(inFlight = false, error = t.message ?: "Control failed", toast = "Control failed") }
            }
        }
    }

    private fun control(intent: PrinterControlIntent) = sendControl {
        container.pandar.control(printerId, intent)
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
