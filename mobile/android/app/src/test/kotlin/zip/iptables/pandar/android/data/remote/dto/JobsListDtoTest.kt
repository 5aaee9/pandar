package zip.iptables.pandar.android.data.remote.dto

import org.junit.Assert.assertEquals
import org.junit.Test
import zip.iptables.pandar.android.data.remote.appJson

class JobsListDtoTest {

    @Test fun parses_active_and_terminal_jobs() {
        val json = """
        {"jobs":[
          {"id":"j1","tenant_id":"t1","printer_id":"p1","agent_id":"a1","artifact_id":"art1","command_id":"c1",
           "status":"acknowledged","error":null,"created_at":"2026-07-05T10:00:00Z","updated_at":"2026-07-05T10:01:00Z",
           "print":{"status":"running","printer_state":null,"progress_percent":42,"remaining_time_minutes":88,"current_layer":120,"total_layers":300,"active_file":null,"last_progress_percent":null,"last_layer":null,"error":null,"started_at":null,"finished_at":null,"updated_at":"2026-07-05T10:05:00Z"},
           "command":{"id":"c1","kind":"dispatch_print","status":"acknowledged"},
           "artifact":{"id":"art1","tenant_id":"t1","filename":"benchy.3mf","content_type":"model/3mf","size_bytes":12345,"metadata":null,"created_at":"2026-07-05T09:59:00Z"},
           "material":{"ams_mapping":null,"ams_mapping2":null,"ams_mapping_info":null,"filament_usage":[]}},
          {"id":"j2","tenant_id":"t1","printer_id":"p1","agent_id":"a1","artifact_id":"art2","command_id":"c2",
           "status":"succeeded","error":null,"created_at":"2026-07-04T10:00:00Z","updated_at":"2026-07-04T12:00:00Z",
           "print":{"status":"completed","printer_state":null,"progress_percent":100,"remaining_time_minutes":0,"current_layer":null,"total_layers":null,"active_file":null,"last_progress_percent":null,"last_layer":null,"error":null,"started_at":null,"finished_at":"2026-07-04T12:00:00Z","updated_at":"2026-07-04T12:00:00Z"},
           "command":{"id":"c2","kind":"print_project_file","status":"succeeded"},
           "artifact":{"id":"art2","tenant_id":"t1","filename":"cube.gcode.3mf","content_type":"model/3mf","size_bytes":999,"metadata":null,"created_at":"2026-07-04T09:59:00Z"},
           "material":{"ams_mapping":null,"ams_mapping2":null,"ams_mapping_info":null,"filament_usage":[]}}
        ]}
        """.trimIndent()

        val dto = appJson.decodeFromString<JobListDto>(json)
        assertEquals(2, dto.jobs.size)
        val active = dto.jobs[0].toDomain()
        assertEquals("j1", active.id)
        assertEquals("acknowledged", active.status)
        assertEquals(42, active.print.progressPercent)
        assertEquals(88, active.print.remainingTimeMinutes)
        assertEquals(120, active.print.currentLayer)
        assertEquals(300, active.print.totalLayers)
        assertEquals("benchy.3mf", active.artifact.filename)
        val done = dto.jobs[1].toDomain()
        assertEquals("succeeded", done.status)
        assertEquals(100, done.print.progressPercent)
    }
}
