package zip.iptables.pandar.android.data.remote.dto

import org.junit.Assert.assertEquals
import org.junit.Test
import zip.iptables.pandar.android.data.remote.appJson

class JobsListDtoTest {

    @Test fun parses_active_and_terminal_jobs() {
        val json = """
        {"jobs":[
          {"id":"j1","printer_id":"p1","agent_id":"a1","artifact_id":"art1","command_id":"c1",
           "status":"dispatched","created_at":"2026-07-05T10:00:00Z","updated_at":"2026-07-05T10:01:00Z",
           "print":{"status":"running","progress_percent":42,"remaining_time_minutes":88,"current_layer":120,"total_layers":300,"updated_at":"2026-07-05T10:05:00Z"},
           "command":{"id":"c1","kind":"dispatch_print","status":"acknowledged"},
           "artifact":{"id":"art1","tenant_id":"t1","filename":"benchy.3mf","content_type":"model/3mf","size_bytes":12345,"created_at":"2026-07-05T09:59:00Z"}},
          {"id":"j2","printer_id":"p1","agent_id":"a1","artifact_id":"art2","command_id":"c2",
           "status":"completed","created_at":"2026-07-04T10:00:00Z","updated_at":"2026-07-04T12:00:00Z",
           "print":{"status":"completed","progress_percent":100,"remaining_time_minutes":0,"updated_at":"2026-07-04T12:00:00Z","finished_at":"2026-07-04T12:00:00Z"},
           "artifact":{"id":"art2","tenant_id":"t1","filename":"cube.gcode.3mf","content_type":"model/3mf","size_bytes":999,"created_at":"2026-07-04T09:59:00Z"}}
        ]}
        """.trimIndent()

        val dto = appJson.decodeFromString<JobListDto>(json)
        assertEquals(2, dto.jobs.size)
        val active = dto.jobs[0].toDomain()
        assertEquals("j1", active.id)
        assertEquals("dispatched", active.status)
        assertEquals(42, active.print.progressPercent)
        assertEquals(88, active.print.remainingTimeMinutes)
        assertEquals(120, active.print.currentLayer)
        assertEquals(300, active.print.totalLayers)
        assertEquals("benchy.3mf", active.artifact.filename)
        val done = dto.jobs[1].toDomain()
        assertEquals("completed", done.status)
        assertEquals(100, done.print.progressPercent)
    }
}
