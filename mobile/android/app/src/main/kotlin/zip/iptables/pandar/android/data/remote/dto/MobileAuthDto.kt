package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class MobileTicketExchangeRequest(
    val ticket: String,
)

@Serializable
data class MobileTicketExchangeResponse(
    val token: String,
    @SerialName("expires_at") val expiresAt: String,
    val profile: MobileAuthProfileDto,
)

@Serializable
data class MobileAuthProfileDto(
    @SerialName("user_id") val userId: String,
    @SerialName("user_name") val userName: String,
    @SerialName("tenant_id") val tenantId: String,
    @SerialName("tenant_name") val tenantName: String,
)
