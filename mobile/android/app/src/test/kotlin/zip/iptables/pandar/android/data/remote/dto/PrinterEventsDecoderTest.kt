package zip.iptables.pandar.android.data.remote.dto

import org.junit.Assert.assertTrue
import org.junit.Test
import zip.iptables.pandar.android.data.remote.appJson

class PrinterEventsDecoderTest {

    @Test fun decodes_printer_snapshot_event() {
        val json = """
        {"type":"printer_snapshot","printer":{
          "id":"p1","tenant_id":"t1","agent_id":"a1","serial_number":"SN001","name":"A",
          "model":null,"status":"idle","last_seen_at":"x","created_at":"y","materials":null}}
        """.trimIndent()

        val event = appJson.decodeFromString<PrinterEventDto>(json)
        assertTrue(event is PrinterEventDto.PrinterSnapshot)
        val snap = event as PrinterEventDto.PrinterSnapshot
        assertTrue(snap.printer.id == "p1")
    }

    @Test fun decodes_job_progress_event() {
        val json = """
        {"type":"job_progress","job":{
          "id":"j1","printer_id":"p1","agent_id":"a1","artifact_id":"art1","command_id":"c1",
          "status":"dispatched","created_at":"a","updated_at":"b",
          "print":{"status":"running","progress_percent":10},
          "artifact":{"id":"art1","tenant_id":"t1","filename":"f.3mf","content_type":"model/3mf","size_bytes":1,"created_at":"c"}}}
        """.trimIndent()

        val event = appJson.decodeFromString<PrinterEventDto>(json)
        assertTrue(event is PrinterEventDto.JobProgress)
        assertTrue((event as PrinterEventDto.JobProgress).job.id == "j1")
    }

    @Test fun decodes_command_result_event() {
        val json = """
        {"type":"command_result","command":{
          "id":"c1","tenant_id":"t1","agent_id":"a1","printer_id":"p1","kind":"printer_operation",
          "status":"completed","payload_json":"{}","created_at":"a","updated_at":"b"}}
        """.trimIndent()

        val event = appJson.decodeFromString<PrinterEventDto>(json)
        assertTrue(event is PrinterEventDto.CommandResult)
        assertTrue((event as PrinterEventDto.CommandResult).command.id == "c1")
    }
}
