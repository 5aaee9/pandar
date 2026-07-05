package zip.iptables.pandar.android.data.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

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
                KEY_OIDC_DISCOVERY_URL to "https://idp/.well-known/openid-configuration",
                KEY_OIDC_CLIENT_ID to "cid",
                KEY_OIDC_SCOPES to "openid,profile",
                KEY_ACCESS_TOKEN to "AT",
                KEY_REFRESH_TOKEN to "RT",
            ),
            tokenExpiresAt = 123456789L,
        )
        assertEquals("https://hub.example/", snapshot.hubBaseUrl)
        assertEquals("t-1", snapshot.tenantId)
        assertEquals("AT", snapshot.accessToken)
        assertEquals(123456789L, snapshot.tokenExpiresAtEpochMillis)
        assertEquals(true, snapshot.hasOidcConfig)
    }

    @Test fun scopes_round_trip() {
        assertEquals(listOf("openid", "profile", "email"), scopesToList("openid, profile,email"))
        assertEquals(emptyList<String>(), scopesToList(null))
        assertEquals(emptyList<String>(), scopesToList(""))
        assertEquals("openid,profile", listToScopes(listOf("openid", "profile")))
    }
}
