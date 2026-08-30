package zip.iptables.pandar.android.data.settings

import zip.iptables.pandar.android.data.remote.HubSession

data class SettingsSnapshot(
    val hubBaseUrl: String? = null,
    val tenantId: String? = null,
    val accessToken: String? = null,
    val tokenExpiresAtEpochMillis: Long? = null,
) {
    val hasHubConfig: Boolean
        get() = !hubBaseUrl.isNullOrEmpty()
}

internal fun SettingsSnapshot.clearSessionIfMatches(expected: HubSession): SettingsSnapshot {
    val current = HubSession.create(hubBaseUrl, tenantId, accessToken)
    return if (current == expected) {
        copy(accessToken = null, tokenExpiresAtEpochMillis = null)
    } else {
        this
    }
}
