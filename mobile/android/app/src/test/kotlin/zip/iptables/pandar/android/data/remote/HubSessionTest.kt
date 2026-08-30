package zip.iptables.pandar.android.data.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HubSessionTest {
    @Test
    fun `session requires one complete secure identity`() {
        assertNull(HubSession.create(null, "tenant-1", "token-1"))
        assertNull(HubSession.create("https://hub.example", null, "token-1"))
        assertNull(HubSession.create("https://hub.example", "tenant-1", null))
        assertNull(HubSession.create("http://hub.example", "tenant-1", "token-1"))
    }

    @Test
    fun `session owns websocket location and authorization identity`() {
        val session = HubSession.create(
            "https://hub.example/pandar",
            " tenant-1 ",
            " token-1 ",
        )!!

        assertEquals("tenant-1", session.tenantId)
        assertEquals("token-1", session.accessToken)
        assertEquals(false, session.toString().contains("token-1"))
        assertEquals(
            "wss://hub.example/pandar/api/v1/tenants/tenant-1/printer-events",
            session.printerEventsUrl.toString(),
        )
    }
}
