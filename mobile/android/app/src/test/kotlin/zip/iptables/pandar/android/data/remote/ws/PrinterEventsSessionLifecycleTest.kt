package zip.iptables.pandar.android.data.remote.ws

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.ApiModule
import zip.iptables.pandar.android.data.remote.HubSession
import zip.iptables.pandar.android.data.remote.HubSessionContext
import zip.iptables.pandar.android.data.remote.dto.PrinterEventDto

class PrinterEventsSessionLifecycleTest {
    private var nextSessionEpoch = 0L

    @Test
    fun `session changes replace the authenticated socket and sign out stops it`() {
        runBlocking {
            val server = MockWebServer()
            server.enqueue(webSocketResponse())
            server.enqueue(webSocketResponse())
            server.enqueue(webSocketResponse())
            server.start()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
            val sessions = MutableStateFlow<HubSessionContext?>(null)
            val repository = repository()

            try {
                repository.start(scope, sessions)
                sessions.value = session(server, "tenant-1", "token-1")
                val first = server.takeRequest(5, TimeUnit.SECONDS)
                assertNotNull(first)
                assertEquals("/api/v1/tenants/tenant-1/printer-events", first!!.path)
                assertEquals("Bearer token-1", first.getHeader("Authorization"))
                withTimeout(5_000) { repository.liveState.first { it == LiveState.CONNECTED } }

                sessions.value = session(server, "tenant-2", "token-2")
                val second = server.takeRequest(5, TimeUnit.SECONDS)
                assertNotNull(second)
                assertEquals("/api/v1/tenants/tenant-2/printer-events", second!!.path)
                assertEquals("Bearer token-2", second.getHeader("Authorization"))
                withTimeout(5_000) { repository.liveState.first { it == LiveState.CONNECTED } }

                repository.reconnect()
                val third = server.takeRequest(5, TimeUnit.SECONDS)
                assertNotNull(third)
                assertEquals("/api/v1/tenants/tenant-2/printer-events", third!!.path)
                assertEquals("Bearer token-2", third.getHeader("Authorization"))

                sessions.value = null
                withTimeout(5_000) { repository.liveState.first { it == LiveState.DISCONNECTED } }
            } finally {
                repository.stop()
                scope.cancel()
                server.shutdown()
            }
        }
    }

    @Test
    fun `buffered frames from a replaced session are rejected`() {
        runBlocking {
            val server = MockWebServer()
            val firstSocket = RecordingWebSocketListener()
            val secondSocket = RecordingWebSocketListener()
            server.enqueue(webSocketResponse(firstSocket))
            server.enqueue(webSocketResponse(secondSocket))
            server.start()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
            val sessions = MutableStateFlow<HubSessionContext?>(null)
            val repository = repository()
            val frameReceived = CountDownLatch(1)
            val releaseFrame = CountDownLatch(1)
            val currentFrameApplied = CountDownLatch(1)
            val applied = AtomicInteger()

            try {
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    repository.events.collect { frame ->
                        frameReceived.countDown()
                        releaseFrame.await()
                        repository.consumeIfCurrent(frame) {
                            applied.incrementAndGet()
                            currentFrameApplied.countDown()
                        }
                    }
                }
                repository.start(scope, sessions)
                sessions.value = session(server, "tenant-1", "token-1")
                assertNotNull(server.takeRequest(5, TimeUnit.SECONDS))
                assertEquals(true, firstSocket.opened.await(5, TimeUnit.SECONDS))
                firstSocket.socket.get().send(PRINTER_EVENT)
                assertEquals(true, frameReceived.await(5, TimeUnit.SECONDS))

                sessions.value = session(server, "tenant-2", "token-2")
                assertNotNull(server.takeRequest(5, TimeUnit.SECONDS))
                assertEquals(true, secondSocket.opened.await(5, TimeUnit.SECONDS))
                releaseFrame.countDown()
                delay(200)
                assertEquals(0, applied.get())

                secondSocket.socket.get().send(PRINTER_EVENT)
                assertEquals(true, currentFrameApplied.await(5, TimeUnit.SECONDS))
                assertEquals(1, applied.get())
            } finally {
                releaseFrame.countDown()
                repository.stop()
                scope.cancel()
                server.shutdown()
            }
        }
    }

    @Test
    fun `queued frame survives a socket reconnect within the same session context`() {
        runBlocking {
            val server = MockWebServer()
            val firstSocket = RecordingWebSocketListener()
            val secondSocket = RecordingWebSocketListener()
            server.enqueue(webSocketResponse(firstSocket))
            server.enqueue(webSocketResponse(secondSocket))
            server.start()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
            val sessions = MutableStateFlow<HubSessionContext?>(null)
            val repository = repository()
            val frameReceived = CountDownLatch(1)
            val releaseFrame = CountDownLatch(1)
            val applied = CountDownLatch(1)

            try {
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    repository.events.collect { frame ->
                        frameReceived.countDown()
                        releaseFrame.await()
                        repository.consumeIfCurrent(frame) { applied.countDown() }
                    }
                }
                repository.start(scope, sessions)
                sessions.value = session(server, "tenant-1", "token-1")
                assertNotNull(server.takeRequest(5, TimeUnit.SECONDS))
                assertEquals(true, firstSocket.opened.await(5, TimeUnit.SECONDS))
                firstSocket.socket.get().send(PRINTER_EVENT)
                assertEquals(true, frameReceived.await(5, TimeUnit.SECONDS))

                repository.reconnect()
                assertNotNull(server.takeRequest(5, TimeUnit.SECONDS))
                assertEquals(true, secondSocket.opened.await(5, TimeUnit.SECONDS))
                releaseFrame.countDown()

                assertEquals(true, applied.await(5, TimeUnit.SECONDS))
            } finally {
                releaseFrame.countDown()
                repository.stop()
                scope.cancel()
                server.shutdown()
            }
        }
    }

    @Test
    fun `event buffer overflow requests a REST resynchronization`() {
        runBlocking {
            val server = MockWebServer()
            val socket = RecordingWebSocketListener()
            server.enqueue(webSocketResponse(socket))
            server.start()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
            val sessions = MutableStateFlow<HubSessionContext?>(null)
            val repository = repository()
            val firstFrame = CompletableDeferred<Unit>()
            val releaseFrames = CompletableDeferred<Unit>()
            val resync = CompletableDeferred<HubSessionContext>()
            val recoveredCommand = CompletableDeferred<PrinterEventsRepository.SessionEvent>()

            try {
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    repository.events.collect {
                        firstFrame.complete(Unit)
                        releaseFrames.await()
                    }
                }
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    repository.resyncRequests.collect { resync.complete(it) }
                }
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    repository.commandRecoveryRequests.collect {
                        repository.drainDroppedCommands().firstOrNull()?.let {
                            recoveredCommand.complete(it)
                        }
                    }
                }
                repository.start(scope, sessions)
                val session = session(server, "tenant-1", "token-1")
                sessions.value = session
                assertNotNull(server.takeRequest(5, TimeUnit.SECONDS))
                assertEquals(true, socket.opened.await(5, TimeUnit.SECONDS))

                socket.socket.get().send(PRINTER_EVENT)
                withTimeout(5_000) { firstFrame.await() }
                repeat(80) { socket.socket.get().send(PRINTER_EVENT) }
                socket.socket.get().send(COMMAND_EVENT)

                assertEquals(session, withTimeout(5_000) { resync.await() })
                val recovered = withTimeout(5_000) { recoveredCommand.await() }
                val command = recovered.event as PrinterEventDto.CommandResult
                assertEquals("completed", command.command.status)
            } finally {
                releaseFrames.complete(Unit)
                repository.stop()
                scope.cancel()
                server.shutdown()
            }
        }
    }

    @Test
    fun `rejected authorization invalidates once and waits for a new session`() {
        runBlocking {
            val server = MockWebServer()
            server.enqueue(MockResponse().setResponseCode(401))
            server.start()
            val invalidations = AtomicInteger()
            val invalidationStarted = CompletableDeferred<Unit>()
            val releaseInvalidation = CompletableDeferred<Unit>()
            val invalidationFinished = CompletableDeferred<Unit>()
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
            val sessions = MutableStateFlow<HubSessionContext?>(null)
            val repository = repository { _ ->
                invalidationStarted.complete(Unit)
                releaseInvalidation.await()
                invalidations.incrementAndGet()
                invalidationFinished.complete(Unit)
            }

            try {
                repository.start(scope, sessions)
                sessions.value = session(server, "tenant-1", "expired-token")

                val request = server.takeRequest(5, TimeUnit.SECONDS)
                assertNotNull(request)
                assertEquals(listOf("Bearer expired-token"), request!!.headers.values("Authorization"))
                withTimeout(5_000) { invalidationStarted.await() }
                repository.reconnect()
                delay(200)
                assertEquals(1, server.requestCount)
                assertEquals(false, invalidationFinished.isCompleted)
                releaseInvalidation.complete(Unit)
                withTimeout(5_000) { invalidationFinished.await() }
                delay(1_200)
                repository.reconnect()
                delay(200)
                assertEquals(1, invalidations.get())
                assertEquals(1, server.requestCount)
                assertEquals(LiveState.DISCONNECTED, repository.liveState.value)
            } finally {
                releaseInvalidation.complete(Unit)
                repository.stop()
                scope.cancel()
                server.shutdown()
            }
        }
    }

    private fun repository(
        invalidateSession: suspend (HubSession) -> Unit = { _ -> },
    ) = PrinterEventsRepository(
        client = ApiModule.webSocketHttp(),
        tokenRefresher = { false },
        invalidateSession = invalidateSession,
        logger = NoOpLogger,
    )

    private fun session(
        server: MockWebServer,
        tenantId: String,
        token: String,
    ): HubSessionContext {
        nextSessionEpoch += 1
        return HubSessionContext(
            HubSession.create(server.url("/").toString(), tenantId, token)!!,
            nextSessionEpoch,
        )
    }

    private fun webSocketResponse(
        listener: WebSocketListener = object : WebSocketListener() {},
    ) = MockResponse().withWebSocketUpgrade(listener)

    private class RecordingWebSocketListener : WebSocketListener() {
        val opened = CountDownLatch(1)
        val socket = AtomicReference<WebSocket>()

        override fun onOpen(webSocket: WebSocket, response: Response) {
            socket.set(webSocket)
            opened.countDown()
        }
    }

    companion object {
        private val COMMAND_EVENT = """
            {"type":"command_result","command":{
              "id":"command-1","tenant_id":"tenant-1","agent_id":"agent-1",
              "printer_id":"p1","kind":"printer_operation","status":"completed",
              "payload_json":"{}","created_at":"created","updated_at":"updated"}}
        """.trimIndent()

        private val PRINTER_EVENT = """
            {"type":"printer_snapshot","printer":{
              "id":"p1","tenant_id":"t1","agent_id":"a1","serial_number":"SN001",
              "name":"A","model":null,"status":"idle","last_seen_at":"x",
              "created_at":"y","materials":null}}
        """.trimIndent()
    }
}

private object NoOpLogger : Logger {
    override fun d(t: Throwable?, msg: () -> String) = Unit
    override fun w(t: Throwable?, msg: () -> String) = Unit
    override fun e(t: Throwable?, msg: () -> String) = Unit
}
