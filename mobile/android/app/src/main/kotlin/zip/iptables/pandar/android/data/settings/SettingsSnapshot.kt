package zip.iptables.pandar.android.data.settings

data class SettingsSnapshot(
    val hubBaseUrl: String? = null,
    val tenantId: String? = null,
    val oidcDiscoveryUrl: String? = null,
    val oidcClientId: String? = null,
    val oidcScopes: String? = null,
    val oidcRedirectUri: String? = null,
    val accessToken: String? = null,
    val refreshToken: String? = null,
    val tokenExpiresAtEpochMillis: Long? = null,
) {
    val hasOidcConfig: Boolean
        get() = !oidcDiscoveryUrl.isNullOrEmpty() && !oidcClientId.isNullOrEmpty()
    val hasHubConfig: Boolean
        get() = !hubBaseUrl.isNullOrEmpty() && !tenantId.isNullOrEmpty()
}
