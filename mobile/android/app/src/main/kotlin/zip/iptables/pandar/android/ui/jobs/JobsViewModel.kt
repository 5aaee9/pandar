package zip.iptables.pandar.android.ui.jobs

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job as CoroutineJob
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
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.PandarState

data class JobsUiState(
    val loading: Boolean = true,
    val jobs: List<Job> = emptyList(),
    val error: String? = null,
    val toast: String? = null,
    val inFlightId: String? = null,
)

internal data class JobsRequestState(
    val sessionGeneration: Long = 0,
    val loading: Boolean = true,
    val error: String? = null,
    val toast: String? = null,
    val inFlightId: String? = null,
)

internal fun jobsUiState(domain: PandarState, request: JobsRequestState): JobsUiState {
    val current = request.takeIf { it.sessionGeneration == domain.sessionGeneration }
        ?: JobsRequestState(sessionGeneration = domain.sessionGeneration)
    return JobsUiState(
        loading = current.loading,
        jobs = domain.jobs,
        error = current.error,
        toast = current.toast,
        inFlightId = current.inFlightId,
    )
}

class JobsViewModel(private val pandar: PandarDataSource) : ViewModel() {
    private val request = MutableStateFlow(
        JobsRequestState(sessionGeneration = pandar.state.value.sessionGeneration),
    )
    private var loadJob: CoroutineJob? = null
    private var actionJob: CoroutineJob? = null

    val state: StateFlow<JobsUiState> = combine(
        pandar.state,
        request,
        ::jobsUiState,
    ).stateIn(
        viewModelScope,
        SharingStarted.Eagerly,
        jobsUiState(pandar.state.value, request.value),
    )

    init {
        viewModelScope.launch {
            pandar.state
                .map { it.sessionGeneration to it.hasSession }
                .distinctUntilChanged()
                .collect { (generation, hasSession) ->
                    if (request.value.sessionGeneration != generation) {
                        loadJob?.cancel()
                        actionJob?.cancel()
                        request.value = JobsRequestState(sessionGeneration = generation)
                        if (hasSession) load()
                    }
                }
        }
        if (pandar.state.value.hasSession) load()
    }

    fun load() {
        loadJob?.cancel()
        val generation = pandar.state.value.sessionGeneration
        loadJob = viewModelScope.launch { loadNow(generation) }
    }

    fun retry(jobId: String) {
        val generation = pandar.state.value.sessionGeneration
        actionJob = viewModelScope.launch {
            updateRequest(generation) { it.copy(inFlightId = jobId, toast = null) }
            try {
                val command = pandar.retry(jobId)
                updateRequest(generation) {
                    it.copy(
                        inFlightId = null,
                        toast = "Retry dispatched: ${command.status}",
                    )
                }
                loadNow(generation)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                updateRequest(generation) {
                    it.copy(
                        inFlightId = null,
                        toast = "Retry failed: ${error.message}",
                    )
                }
            }
        }
    }

    fun reprint(jobId: String) {
        val generation = pandar.state.value.sessionGeneration
        actionJob = viewModelScope.launch {
            updateRequest(generation) { it.copy(inFlightId = jobId, toast = null) }
            try {
                val command = pandar.reprint(jobId)
                updateRequest(generation) {
                    it.copy(
                        inFlightId = null,
                        toast = "Reprint queued: ${command.status}",
                    )
                }
                loadNow(generation)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                updateRequest(generation) {
                    it.copy(
                        inFlightId = null,
                        toast = "Reprint failed: ${error.message}",
                    )
                }
            }
        }
    }

    private suspend fun loadNow(generation: Long) {
        updateRequest(generation) { it.copy(loading = true, error = null) }
        try {
            pandar.refreshJobs()
            updateRequest(generation) { it.copy(loading = false) }
        } catch (error: CancellationException) {
            throw error
        } catch (error: Throwable) {
            updateRequest(generation) {
                it.copy(
                    loading = false,
                    error = error.message ?: "Failed to load jobs",
                )
            }
        }
    }

    private fun updateRequest(
        generation: Long,
        transform: (JobsRequestState) -> JobsRequestState,
    ) {
        request.update { current ->
            if (current.sessionGeneration == generation) transform(current) else current
        }
    }
}
