package zip.iptables.pandar.android.data.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import zip.iptables.pandar.android.data.remote.HubSession

class SettingsMappingTest {

    @Test fun empty_map_produces_empty_snapshot() {
        val snapshot = settingsToSnapshot(emptyMap(), null)
        assertNull(snapshot.hubBaseUrl)
        assertNull(snapshot.tenantId)
        assertNull(snapshot.accessToken)
        assertNull(snapshot.tokenExpiresAtEpochMillis)
    }

    @Test fun set_keys_round_trip() {
        val snapshot = settingsToSnapshot(
            mapOf(
                KEY_HUB_BASE_URL to "https://hub.example/",
                KEY_TENANT_ID to "t-1",
                KEY_ACCESS_TOKEN to "AT",
            ),
            tokenExpiresAt = 123456789L,
        )
        assertEquals("https://hub.example/", snapshot.hubBaseUrl)
        assertEquals("t-1", snapshot.tenantId)
        assertEquals("AT", snapshot.accessToken)
        assertEquals(123456789L, snapshot.tokenExpiresAtEpochMillis)
        assertEquals(true, snapshot.hasHubConfig)
    }

    @Test fun rejected_session_only_clears_the_matching_token_identity() {
        val current = SettingsSnapshot(
            hubBaseUrl = "https://hub.example",
            tenantId = "tenant-1",
            accessToken = "token-2",
            tokenExpiresAtEpochMillis = 2L,
        )
        val rejected = HubSession.create("https://hub.example", "tenant-1", "token-1")!!
        assertEquals(current, current.clearSessionIfMatches(rejected))

        val matching = HubSession.create("https://hub.example", "tenant-1", "token-2")!!
        assertEquals(
            current.copy(accessToken = null, tokenExpiresAtEpochMillis = null),
            current.clearSessionIfMatches(matching),
        )
    }

    @Test fun hub_url_only_is_configured_before_login() {
        val snapshot = settingsToSnapshot(
            mapOf(KEY_HUB_BASE_URL to "https://hub.example/"),
            tokenExpiresAt = null,
        )

        assertEquals(true, snapshot.hasHubConfig)
        assertNull(snapshot.tenantId)
        assertNull(snapshot.accessToken)
    }
}
