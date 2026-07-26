package zip.iptables.pandar.android.data.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HubUrlTest {
    @Test
    fun `requires https except for loopback development hubs`() {
        assertEquals("https://hub.example/", secureHubHttpUrl("https://hub.example")?.toString())
        assertEquals("http://127.0.0.1:8080/", secureHubHttpUrl("http://127.0.0.1:8080")?.toString())
        assertNull(secureHubHttpUrl("http://hub.example"))
        assertNull(secureHubHttpUrl("http://192.168.1.10:8080"))
    }
}
