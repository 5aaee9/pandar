package zip.iptables.pandar.android.ui.jobs

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import zip.iptables.pandar.android.data.remote.ws.LiveState
import zip.iptables.pandar.android.data.repository.PandarDataSource
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.PandarState
import zip.iptables.pandar.android.domain.model.PrinterControlIntent
import kotlin.coroutines.Continuation
import kotlin.coroutines.suspendCoroutine

@OptIn(ExperimentalCoroutinesApi::class)
class JobsViewModelSessionTest {
    @get:Rule
    val mainDispatcherRule = MainDispatcherRule()

    @Test
    fun `late success from replaced session cannot complete new request state`() = runTest {
        val source = DelayedJobsDataSource()
        val viewModel = JobsViewModel(source)
        advanceUntilIdle()

        source.replaceSession()
        advanceUntilIdle()
        source.completeRefresh(Result.success(Unit))
        advanceUntilIdle()

        assertTrue(viewModel.state.value.loading)
        assertEquals(null, viewModel.state.value.error)
        assertTrue(viewModel.state.value.jobs.isEmpty())
    }

    @Test
    fun `late error from replaced session cannot overwrite new request state`() = runTest {
        val source = DelayedJobsDataSource()
        val viewModel = JobsViewModel(source)
        advanceUntilIdle()

        source.replaceSession()
        advanceUntilIdle()
        source.completeRefresh(Result.failure(IllegalStateException("old session failed")))
        advanceUntilIdle()

        assertTrue(viewModel.state.value.loading)
        assertEquals(null, viewModel.state.value.error)
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class MainDispatcherRule(
    private val dispatcher: TestDispatcher = StandardTestDispatcher(),
) : TestWatcher() {
    override fun starting(description: Description) {
        Dispatchers.setMain(dispatcher)
    }

    override fun finished(description: Description) {
        Dispatchers.resetMain()
    }
}

private class DelayedJobsDataSource : PandarDataSource {
    override val state = MutableStateFlow(
        PandarState(sessionGeneration = 1, hasSession = true),
    )
    override val liveState = MutableStateFlow(LiveState.CONNECTED)
    private var refreshContinuation: Continuation<Unit>? = null

    override suspend fun refreshJobs() {
        suspendCoroutine { refreshContinuation = it }
    }

    fun replaceSession() {
        state.value = PandarState(sessionGeneration = 2, hasSession = false)
    }

    fun completeRefresh(result: Result<Unit>) {
        requireNotNull(refreshContinuation).resumeWith(result)
        refreshContinuation = null
    }

    override suspend fun refreshPrinters() = unsupported()
    override suspend fun refreshPrinter(id: String) = unsupported()
    override suspend fun agents(): List<Agent> = unsupported()
    override suspend fun control(
        printerId: String,
        intent: PrinterControlIntent,
    ): Command = unsupported()
    override suspend fun retry(jobId: String): Command = unsupported()
    override suspend fun reprint(jobId: String): Command = unsupported()

    private fun unsupported(): Nothing = error("not used by this test")
}
