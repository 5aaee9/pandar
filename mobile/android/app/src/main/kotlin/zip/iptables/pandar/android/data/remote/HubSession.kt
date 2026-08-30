package zip.iptables.pandar.android.data.remote

import okhttp3.HttpUrl

data class HubSessionContext(
    val identity: HubSession,
    val epoch: Long,
) {
    override fun toString(): String =
        "HubSessionContext(identity=$identity, epoch=$epoch)"
}

data class HubSession(
    val baseUrl: HttpUrl,
    val tenantId: String,
    val accessToken: String,
) : TokenProvider {
    val printerEventsUrl: String
        get() {
            val httpBase = baseUrl.toString().trimEnd('/')
            val wsBase = if (baseUrl.isHttps) {
                "wss" + httpBase.removePrefix("https")
            } else {
                "ws" + httpBase.removePrefix("http")
            }
            return "$wsBase/api/v1/tenants/$tenantId/printer-events"
        }

    override fun currentToken(): String = accessToken

    override fun toString(): String =
        "HubSession(baseUrl=$baseUrl, tenantId=$tenantId, accessToken=[REDACTED])"

    companion object {
        fun create(
            hubBaseUrl: String?,
            tenantId: String?,
            accessToken: String?,
        ): HubSession? {
            val baseUrl = secureHubHttpUrl(hubBaseUrl) ?: return null
            val tenant = tenantId?.trim()?.takeIf(String::isNotEmpty) ?: return null
            val token = accessToken?.trim()?.takeIf(String::isNotEmpty) ?: return null
            return HubSession(baseUrl, tenant, token)
        }
    }
}
