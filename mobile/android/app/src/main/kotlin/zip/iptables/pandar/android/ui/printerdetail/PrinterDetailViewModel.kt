package zip.iptables.pandar.android.ui.printerdetail

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.data.remote.dto.AmsLoadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.AmsRereadRfidRequest
import zip.iptables.pandar.android.data.remote.dto.AmsUnloadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto
import zip.iptables.pandar.android.data.remote.dto.SetBedTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberLightRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetHotendTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.toDomain
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.Printer

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
        load()
        viewModelScope.launch {
            container.pandar.events.collect { event ->
                when (event) {
                    is PrinterEventDto.PrinterSnapshot -> {
                        if (event.printer.id == printerId) {
                            _state.update { it.copy(printer = event.printer.toDomain()) }
                        }
                    }
                    is PrinterEventDto.CommandResult -> {
                        val cmd = event.command.toDomain()
                        if (cmd.printerId == printerId) {
                            _state.update {
                                it.copy(
                                    lastCommandId = cmd.id,
                                    lastCommandStatus = cmd.status,
                                    toast = "Command ${cmd.id.take(8)}: ${cmd.status}",
                                )
                            }
                        }
                    }
                    is PrinterEventDto.JobProgress -> Unit
                }
            }
        }
    }

    fun load() {
        viewModelScope.launch {
            _state.update { it.copy(loading = true, error = null) }
            try {
                _state.update { it.copy(loading = false, printer = container.pandar.printer(printerId)) }
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

    fun pause() = sendControl { container.pandar.pause(printerId) }
    fun resume() = sendControl { container.pandar.resume(printerId) }
    fun stop() = sendControl { container.pandar.stop(printerId) }
    fun toggleLight() = sendControl { container.pandar.toggleLight(printerId) }
    fun setChamberLight(on: Boolean) = sendControl {
        container.pandar.setChamberLight(printerId, on)
    }
    fun setHotend(temperatureCelsius: Int, wait: Boolean, extruderId: Int?) = sendControl {
        container.pandar.setHotendTemperature(printerId, SetHotendTemperatureRequest(temperatureCelsius = temperatureCelsius, wait = wait, extruderId = extruderId))
    }
    fun setBed(temperatureCelsius: Int, wait: Boolean) = sendControl {
        container.pandar.setBedTemperature(printerId, SetBedTemperatureRequest(temperatureCelsius = temperatureCelsius, wait = wait))
    }
    fun setChamber(temperatureCelsius: Int, wait: Boolean) = sendControl {
        container.pandar.setChamberTemperature(printerId, SetChamberTemperatureRequest(temperatureCelsius = temperatureCelsius, wait = wait))
    }
    fun amsReread(amsId: Int, slotId: Int) = sendControl {
        container.pandar.amsRereadRfid(printerId, AmsRereadRfidRequest(amsId = amsId, slotId = slotId))
    }
    fun amsLoad(request: AmsLoadFilamentRequest) = sendControl {
        container.pandar.amsLoadFilament(printerId, request)
    }
    fun amsUnload(request: AmsUnloadFilamentRequest) = sendControl {
        container.pandar.amsUnloadFilament(printerId, request)
    }
}
