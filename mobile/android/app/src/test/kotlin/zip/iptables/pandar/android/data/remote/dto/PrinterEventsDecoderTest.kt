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

    @Test fun decodes_enriched_printer_snapshot_with_additive_live_fields() {
        val json = """
        {"type":"printer_snapshot","printer":{
          "id":"p1","tenant_id":"t1","agent_id":"a1","serial_number":"20P001","name":"A",
          "model":"A1","status":"running","last_seen_at":"x","created_at":"y",
          "materials":{"ams_units":[],"external_spools":[],"active_tray":null,"observed_at":"m"},
          "state_revision":42,
          "print":{
            "task_generation":3,"error_generation":9,"hms":[{"attr":1,"code":2}],
            "job_state":1,"gcode_state":"PAUSE","task_id":"task-1","subtask_id":"subtask-1",
            "subtask_name":"Benchy","gcode_file":"/cache/plate_1.gcode.3mf",
            "progress_percent":42,"remaining_time_minutes":12,"current_layer":5,"total_layers":100,
            "print_error":83918929,"printer_job_id":"native-job"
          }
        }}
        """.trimIndent()

        val event = appJson.decodeFromString<PrinterEventDto>(json)
        assertTrue(event is PrinterEventDto.PrinterSnapshot)
        val snap = event as PrinterEventDto.PrinterSnapshot
        assertTrue(snap.printer.id == "p1")
        assertTrue(snap.printer.materials?.observedAt == "m")
    }

    @Test fun decodes_job_progress_event() {
        val json = """
        {"type":"job_progress","job":{
          "id":"j1","tenant_id":"t1","printer_id":"p1","agent_id":"a1","artifact_id":"art1","command_id":"c1",
          "status":"acknowledged","created_at":"a","updated_at":"b",
          "print":{"status":"running","progress_percent":10},
          "command":{"id":"c1","kind":"print_project_file","status":"acknowledged"},
          "artifact":{"id":"art1","tenant_id":"t1","filename":"f.3mf","content_type":"model/3mf","size_bytes":1,"created_at":"c"},
          "material":{"ams_mapping":null,"ams_mapping2":null,"ams_mapping_info":null,"filament_usage":[]}}}
        """.trimIndent()

        val event = appJson.decodeFromString<PrinterEventDto>(json)
        assertTrue(event is PrinterEventDto.JobProgress)
        assertTrue((event as PrinterEventDto.JobProgress).job.id == "j1")
    }

    @Test fun decodes_command_result_event() {
        val json = """
        {"type":"command_result","command":{
          "id":"c1","tenant_id":"t1","agent_id":"a1","printer_id":"p1","kind":"printer_operation",
          "status":"succeeded","payload_json":"{}","created_at":"a","updated_at":"b"}}
        """.trimIndent()

        val event = appJson.decodeFromString<PrinterEventDto>(json)
        assertTrue(event is PrinterEventDto.CommandResult)
        assertTrue((event as PrinterEventDto.CommandResult).command.id == "c1")
    }
}
