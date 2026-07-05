package zip.iptables.pandar.android.domain.status

import org.junit.Assert.assertEquals
import org.junit.Test
import zip.iptables.pandar.android.domain.model.Severity

class StatusMetaTest {

    @Test fun success_tokens() {
        listOf("online", "ok", "succeeded", "completed", "running", "printing", "ready")
            .forEach { assertEquals("$it -> SUCCESS", Severity.SUCCESS, statusMeta(it).severity) }
    }

    @Test fun warning_tokens() {
        listOf("warning", "queued", "sent", "acknowledged", "connecting", "problem", "degraded", "pending")
            .forEach { assertEquals("$it -> WARNING", Severity.WARNING, statusMeta(it).severity) }
    }

    @Test fun critical_tokens() {
        listOf("failed", "offline", "unavailable", "error", "down")
            .forEach { assertEquals("$it -> CRITICAL", Severity.CRITICAL, statusMeta(it).severity) }
    }

    @Test fun unknown_does_not_throw() {
        assertEquals(Severity.INFO, statusMeta("flumbus").severity)
        assertEquals(Severity.INFO, statusMeta("").severity)
        assertEquals(Severity.INFO, statusMeta("   ").severity)
    }

    @Test fun case_insensitive() {
        assertEquals(Severity.CRITICAL, statusMeta("OFFLINE").severity)
        assertEquals(Severity.CRITICAL, statusMeta("Offline").severity)
        assertEquals(Severity.WARNING, statusMeta("Problem").severity)
        assertEquals(Severity.SUCCESS, statusMeta("RUNNING").severity)
    }

    @Test fun label_prettified() {
        assertEquals("Running", statusMeta("running").label)
        assertEquals("Needs attention", statusMeta("needs_attention").label)
        assertEquals("Foo bar", statusMeta("foo-bar").label)
        assertEquals("Flumbus", statusMeta("flumbus").label)
        assertEquals("Unknown", statusMeta("").label)
        assertEquals("Unknown", statusMeta("   ").label)
    }
}
