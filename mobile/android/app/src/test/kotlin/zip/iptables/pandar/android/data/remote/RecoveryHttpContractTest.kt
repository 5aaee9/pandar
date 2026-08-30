package zip.iptables.pandar.android.data.remote

import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import zip.iptables.pandar.android.data.remote.dto.RecoveryReasonRequestDto
import zip.iptables.pandar.android.data.remote.dto.ReprintJobRequestDto

class RecoveryHttpContractTest {
    @Test
    fun `recovery sends JSON bodies and follows the returned command id`() = runBlocking {
        val server = MockWebServer()
        server.enqueue(jsonResponse(JOB))
        server.enqueue(jsonResponse(COMMAND, status = 200))
        server.enqueue(jsonResponse(JOB))
        server.start()
        val api = ApiModule.pandarApi(server.url("/"), OkHttpClient())

        try {
            val retried = api.retryDispatch(
                "tenant-1",
                "source-job",
                RecoveryReasonRequestDto(),
            )
            val command = api.getCommand("tenant-1", retried.commandId)
            api.reprint("tenant-1", "source-job", ReprintJobRequestDto())

            val retryRequest = server.takeRequest()
            val commandRequest = server.takeRequest()
            val reprintRequest = server.takeRequest()
            assertEquals(
                "/api/v1/tenants/tenant-1/jobs/source-job/retry-dispatch",
                retryRequest.path,
            )
            assertEquals("{}", retryRequest.body.readUtf8())
            assertTrue(retryRequest.getHeader("Content-Type")!!.startsWith("application/json"))
            assertEquals(
                "/api/v1/tenants/tenant-1/commands/command-1",
                commandRequest.path,
            )
            assertEquals("command-1", command.id)
            assertEquals(
                "/api/v1/tenants/tenant-1/jobs/source-job/reprint",
                reprintRequest.path,
            )
            assertEquals("{}", reprintRequest.body.readUtf8())
        } finally {
            server.shutdown()
        }
    }

    private fun jsonResponse(body: String, status: Int = 201) = MockResponse()
        .setResponseCode(status)
        .setHeader("Content-Type", "application/json")
        .setBody(body)

    companion object {
        private const val JOB = """
            {"id":"job-1","tenant_id":"tenant-1","printer_id":"printer-1","agent_id":"agent-1","artifact_id":"artifact-1","command_id":"command-1","status":"queued","error":null,"created_at":"created","updated_at":"updated","print":{"status":"pending","printer_state":null,"progress_percent":null,"remaining_time_minutes":null,"current_layer":null,"total_layers":null,"active_file":null,"last_progress_percent":null,"last_layer":null,"error":null,"started_at":null,"finished_at":null,"updated_at":null},"command":{"id":"command-1","kind":"print_project_file","status":"queued"},"artifact":{"id":"artifact-1","tenant_id":"tenant-1","filename":"part.3mf","content_type":"model/3mf","size_bytes":1,"metadata":null,"created_at":"created"},"material":{"ams_mapping":null,"ams_mapping2":null,"ams_mapping_info":null,"filament_usage":[]}}
        """
        private const val COMMAND = """
            {"id":"command-1","tenant_id":"tenant-1","agent_id":"agent-1","printer_id":"printer-1","kind":"print_project_file","status":"queued","payload_json":"{}","error":null,"result_json":null,"created_at":"created","updated_at":"updated"}
        """
    }
}
