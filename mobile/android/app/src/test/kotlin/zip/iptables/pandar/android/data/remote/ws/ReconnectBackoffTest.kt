package zip.iptables.pandar.android.data.remote.ws

import org.junit.Assert.assertEquals
import org.junit.Test

class ReconnectBackoffTest {
    private val backoff = PrinterEventsRepository.ReconnectBackoff(
        initialMs = 1_000L,
        maxMs = 30_000L,
    )

    @Test
    fun `failures double the delay up to the maximum`() {
        assertEquals(1_000L, backoff.advanceAfterFailure())
        assertEquals(2_000L, backoff.currentMs)
        assertEquals(2_000L, backoff.advanceAfterFailure())
        assertEquals(4_000L, backoff.currentMs)
    }

    @Test
    fun `delays cap at the maximum instead of growing forever`() {
        repeat(10) { backoff.advanceAfterFailure() }
        assertEquals(30_000L, backoff.currentMs)
        backoff.advanceAfterFailure()
        assertEquals(30_000L, backoff.currentMs)
    }

    @Test
    fun `a healthy session resets the delay for the next reconnect`() {
        repeat(6) { backoff.advanceAfterFailure() }
        assertEquals(30_000L, backoff.currentMs)

        backoff.reset()

        assertEquals(1_000L, backoff.currentMs)
        // The growth sequence restarts from the initial delay.
        backoff.advanceAfterFailure()
        assertEquals(2_000L, backoff.currentMs)
    }
}
