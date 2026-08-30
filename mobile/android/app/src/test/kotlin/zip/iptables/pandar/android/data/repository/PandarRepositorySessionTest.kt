package zip.iptables.pandar.android.data.repository

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.coroutines.Continuation
import kotlin.coroutines.suspendCoroutine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.OkHttpClient
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.ApiModule
import zip.iptables.pandar.android.data.remote.HubApiSession
import zip.iptables.pandar.android.data.remote.HubSession
import zip.iptables.pandar.android.data.remote.HubSessionContext
import zip.iptables.pandar.android.data.remote.PandarApi
import zip.iptables.pandar.android.data.remote.dto.AgentsListDto
import zip.iptables.pandar.android.data.remote.dto.CommandResponseDto
import zip.iptables.pandar.android.data.remote.dto.JobListDto
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeRequest
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeResponse
import zip.iptables.pandar.android.data.remote.dto.PrinterControlRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterDto
import zip.iptables.pandar.android.data.remote.dto.PrinterListDto
import zip.iptables.pandar.android.data.remote.ws.PrinterEventsRepository
import zip.iptables.pandar.android.domain.model.PrinterControlIntent

class PandarRepositorySessionTest {
    private var nextSessionEpoch = 0L

    @Test
    fun `session identity owns API requests and atomically resets cached state`() = runBlocking {
        val server = MockWebServer()
        server.enqueue(jsonResponse(PRINTERS))
        server.enqueue(jsonResponse(EMPTY_JOBS))
        server.start()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val sessions = MutableStateFlow<HubSessionContext?>(null)
        val repository = PandarRepository(
            sessions = sessions,
            apiSession = { context ->
                val identity = context.identity
                val client = ApiModule.okHttp(
                    tokenProvider = identity,
                    tokenRefresher = { false },
                    logger = RepositoryTestLogger,
                )
                HubApiSession(context, ApiModule.pandarApi(identity.baseUrl, client))
            },
            ws = printerEvents(),
            scope = scope,
            logger = RepositoryTestLogger,
        )

        try {
            sessions.value = session(server, "tenant-1", "token-1")
            val firstState = withTimeout(5_000) {
                repository.state.first { it.printers.singleOrNull()?.id == "printer-1" }
            }
            assertTrue(firstState.hasSession)
            val firstGeneration = firstState.sessionGeneration
            val firstPrinters = server.takeRequest(5, TimeUnit.SECONDS)
            val firstJobs = server.takeRequest(5, TimeUnit.SECONDS)
            assertNotNull(firstPrinters)
            assertNotNull(firstJobs)
            assertEquals("/api/v1/tenants/tenant-1/printers", firstPrinters!!.path)
            assertEquals("/api/v1/tenants/tenant-1/jobs", firstJobs!!.path)
            assertEquals(listOf("Bearer token-1"), firstPrinters.headers.values("Authorization"))

            server.enqueue(jsonResponse(COMMAND))
            val command = repository.control("printer-1", PrinterControlIntent.Pause)
            assertEquals(
                command,
                repository.state.value.latestCommandsByPrinter["printer-1"],
            )
            assertEquals(
                "/api/v1/tenants/tenant-1/printers/printer-1/controls",
                server.takeRequest(5, TimeUnit.SECONDS)?.path,
            )

            server.enqueue(jsonResponse(EMPTY_PRINTERS))
            server.enqueue(jsonResponse(EMPTY_JOBS))
            sessions.value = session(server, "tenant-2", "token-2")
            val reset = withTimeout(5_000) {
                repository.state.first { it.sessionGeneration > firstGeneration }
            }
            assertTrue(reset.hasSession)
            assertTrue(reset.printers.isEmpty())
            assertTrue(reset.jobs.isEmpty())
            assertTrue(reset.latestCommandsByPrinter.isEmpty())

            val secondPrinters = server.takeRequest(5, TimeUnit.SECONDS)
            val secondJobs = server.takeRequest(5, TimeUnit.SECONDS)
            assertNotNull(secondPrinters)
            assertNotNull(secondJobs)
            assertEquals("/api/v1/tenants/tenant-2/printers", secondPrinters!!.path)
            assertEquals("/api/v1/tenants/tenant-2/jobs", secondJobs!!.path)
            assertEquals(listOf("Bearer token-2"), secondPrinters.headers.values("Authorization"))

            sessions.value = null
            val signedOut = withTimeout(5_000) {
                repository.state.first { !it.hasSession }
            }
            assertFalse(signedOut.hasSession)
            assertTrue(signedOut.printers.isEmpty())
        } finally {
            scope.cancel()
            server.shutdown()
        }
    }

    @Test
    fun `old response cannot cross an equal A to B to A session cycle`() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val first = sessionIdentity("tenant-a", "same-token", epoch = 1)
        val sessions = MutableStateFlow<HubSessionContext?>(first)
        val api = DelayedControlApi()
        val repository = PandarRepository(
            sessions = sessions,
            apiSession = { context -> HubApiSession(context, api) },
            ws = printerEvents(),
            scope = scope,
            logger = RepositoryTestLogger,
        )

        try {
            val firstState = withTimeout(5_000) { repository.state.first { it.hasSession } }
            val pending = async {
                runCatching {
                    repository.control("printer-1", PrinterControlIntent.Pause)
                }
            }
            withTimeout(5_000) { api.controlStarted.await() }

            sessions.value = sessionIdentity("tenant-b", "other-token", epoch = 2)
            val secondState = withTimeout(5_000) {
                repository.state.first {
                    it.sessionGeneration > firstState.sessionGeneration
                }
            }
            sessions.value = HubSessionContext(first.identity, epoch = 3)
            withTimeout(5_000) {
                repository.state.first {
                    it.sessionGeneration > secondState.sessionGeneration
                }
            }

            api.completeControl(commandResponse())
            val result = pending.await()
            assertTrue(result.exceptionOrNull() is CancellationException)
            assertTrue(repository.state.value.latestCommandsByPrinter.isEmpty())
        } finally {
            scope.cancel()
        }
    }

    @Test
    fun `websocket context waits until repository ownership is installed`() = runBlocking {
        val server = MockWebServer()
        server.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {}))
        server.start()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val sessions = MutableStateFlow<HubSessionContext?>(null)
        val apiFactoryStarted = CountDownLatch(1)
        val releaseApiFactory = CountDownLatch(1)
        val webSocket = printerEvents()
        val repository = PandarRepository(
            sessions = sessions,
            apiSession = { context ->
                apiFactoryStarted.countDown()
                releaseApiFactory.await()
                HubApiSession(context, ImmediateApi())
            },
            ws = webSocket,
            scope = scope,
            logger = RepositoryTestLogger,
        )
        webSocket.start(scope, repository.readySessions)

        try {
            sessions.value = HubSessionContext(
                HubSession.create(server.url("/").toString(), "tenant-1", "token-1")!!,
                epoch = 1,
            )
            assertTrue(apiFactoryStarted.await(5, TimeUnit.SECONDS))
            assertEquals(null, server.takeRequest(200, TimeUnit.MILLISECONDS))

            releaseApiFactory.countDown()
            assertNotNull(server.takeRequest(5, TimeUnit.SECONDS))
        } finally {
            releaseApiFactory.countDown()
            webSocket.stop()
            scope.cancel()
            server.shutdown()
        }
    }

    private fun printerEvents() = PrinterEventsRepository(
        client = OkHttpClient(),
        tokenRefresher = { false },
        invalidateSession = { _ -> },
        logger = RepositoryTestLogger,
    )

    private fun session(
        server: MockWebServer,
        tenant: String,
        token: String,
    ): HubSessionContext {
        nextSessionEpoch += 1
        return HubSessionContext(
            HubSession.create(server.url("/").toString(), tenant, token)!!,
            nextSessionEpoch,
        )
    }

    private fun sessionIdentity(
        tenant: String,
        token: String,
        epoch: Long,
    ) = HubSessionContext(
        HubSession.create("http://127.0.0.1:8080", tenant, token)!!,
        epoch,
    )

    private fun commandResponse() = CommandResponseDto(
        id = "command-1",
        tenantId = "tenant-a",
        agentId = "agent-1",
        printerId = "printer-1",
        kind = "printer_operation",
        status = "sent",
        payloadJson = "{}",
        createdAt = "created",
        updatedAt = "updated",
    )

    private fun jsonResponse(body: String) = MockResponse()
        .setHeader("Content-Type", "application/json")
        .setBody(body)

    companion object {
        private const val PRINTERS = """
            {"printers":[{"id":"printer-1","tenant_id":"tenant-1","agent_id":"agent-1",
            "serial_number":"SN001","name":"Printer","model":null,"status":"idle",
            "last_seen_at":"seen","created_at":"created","materials":null}]}
        """
        private const val EMPTY_PRINTERS = """{"printers":[]}"""
        private const val EMPTY_JOBS = """{"jobs":[]}"""
        private const val COMMAND = """
            {"id":"command-1","tenant_id":"tenant-1","agent_id":"agent-1",
            "printer_id":"printer-1","kind":"printer_operation","status":"queued",
            "payload_json":"{}","created_at":"created","updated_at":"updated"}
        """
    }
}

private open class ImmediateApi : PandarApi {
    override suspend fun listPrinters(tenant: String) = PrinterListDto(emptyList())
    override suspend fun listJobs(tenant: String) = JobListDto(emptyList())
    override suspend fun listAgents(tenant: String) = AgentsListDto(emptyList())
    override suspend fun exchangeMobileLoginTicket(
        body: MobileTicketExchangeRequest,
    ): MobileTicketExchangeResponse = unsupported()
    override suspend fun getPrinter(tenant: String, printer: String): PrinterDto = unsupported()
    override suspend fun control(
        tenant: String,
        printer: String,
        body: PrinterControlRequest,
    ): CommandResponseDto = unsupported()
    override suspend fun retryDispatch(tenant: String, job: String): CommandResponseDto = unsupported()
    override suspend fun reprint(tenant: String, job: String): CommandResponseDto = unsupported()

    protected fun unsupported(): Nothing = error("not used by this test")
}

private class DelayedControlApi : ImmediateApi() {
    val controlStarted = CompletableDeferred<Unit>()
    private var controlContinuation: Continuation<CommandResponseDto>? = null

    override suspend fun control(
        tenant: String,
        printer: String,
        body: PrinterControlRequest,
    ): CommandResponseDto = suspendCoroutine { continuation ->
        controlContinuation = continuation
        controlStarted.complete(Unit)
    }

    fun completeControl(response: CommandResponseDto) {
        requireNotNull(controlContinuation).resumeWith(Result.success(response))
        controlContinuation = null
    }
}

private object RepositoryTestLogger : Logger {
    override fun d(t: Throwable?, msg: () -> String) = Unit
    override fun w(t: Throwable?, msg: () -> String) = Unit
    override fun e(t: Throwable?, msg: () -> String) = Unit
}
