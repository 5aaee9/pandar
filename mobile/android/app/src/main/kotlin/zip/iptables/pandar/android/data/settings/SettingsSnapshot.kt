package zip.iptables.pandar.android.data.settings

data class SettingsSnapshot(
    val hubBaseUrl: String? = null,
    val tenantId: String? = null,
    val accessToken: String? = null,
    val tokenExpiresAtEpochMillis: Long? = null,
) {
    val hasHubConfig: Boolean
        get() = !hubBaseUrl.isNullOrEmpty()
}
