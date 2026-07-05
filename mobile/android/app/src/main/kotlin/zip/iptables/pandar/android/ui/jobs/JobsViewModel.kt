package zip.iptables.pandar.android.ui.jobs

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
import zip.iptables.pandar.android.domain.model.Job

data class JobsUiState(
    val loading: Boolean = true,
    val jobs: List<Job> = emptyList(),
    val error: String? = null,
    val toast: String? = null,
    val inFlightId: String? = null,
)

class JobsViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(JobsUiState())
    val state: StateFlow<JobsUiState> = _state.asStateFlow()

    init {
        load()
        viewModelScope.launch {
            container.pandar.events.collect { event ->
                if (event is PrinterEventDto.JobProgress) {
                    val job = event.job.toDomain()
                    _state.update { state ->
                        val replaced = state.jobs.map { if (it.id == job.id) job else it }
                        state.copy(jobs = if (replaced.any { it.id == job.id }) replaced else replaced + job)
                    }
                }
            }
        }
    }

    fun load() {
        viewModelScope.launch {
            _state.update { it.copy(loading = true, error = null) }
            try {
                _state.update { it.copy(loading = false, jobs = container.pandar.jobs()) }
            } catch (t: Throwable) {
                _state.update { it.copy(loading = false, error = t.message ?: "Failed to load jobs") }
            }
        }
    }

    fun retry(jobId: String) {
        viewModelScope.launch {
            _state.update { it.copy(inFlightId = jobId, toast = null) }
            try {
                val command = container.pandar.retry(jobId)
                _state.update { it.copy(inFlightId = null, toast = "Retry dispatched: ${command.status}") }
                load()
            } catch (t: Throwable) {
                _state.update { it.copy(inFlightId = null, toast = "Retry failed: ${t.message}") }
            }
        }
    }

    fun reprint(jobId: String) {
        viewModelScope.launch {
            _state.update { it.copy(inFlightId = jobId, toast = null) }
            try {
                val command = container.pandar.reprint(jobId)
                _state.update { it.copy(inFlightId = null, toast = "Reprint queued: ${command.status}") }
                load()
            } catch (t: Throwable) {
                _state.update { it.copy(inFlightId = null, toast = "Reprint failed: ${t.message}") }
            }
        }
    }
}
