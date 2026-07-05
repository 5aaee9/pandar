package zip.iptables.pandar.android.data.settings

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

/**
 * Pure mapping between the DataStore preferences map and [SettingsSnapshot].
 * Extracted so it can be unit-tested without a Robolectric/Android context.
 */
internal fun settingsToSnapshot(
    values: Map<String, String?>,
    tokenExpiresAt: Long?,
): SettingsSnapshot = SettingsSnapshot(
    hubBaseUrl = values[KEY_HUB_BASE_URL],
    tenantId = values[KEY_TENANT_ID],
    oidcDiscoveryUrl = values[KEY_OIDC_DISCOVERY_URL],
    oidcClientId = values[KEY_OIDC_CLIENT_ID],
    oidcScopes = values[KEY_OIDC_SCOPES],
    oidcRedirectUri = values[KEY_OIDC_REDIRECT_URI],
    accessToken = values[KEY_ACCESS_TOKEN],
    refreshToken = values[KEY_REFRESH_TOKEN],
    tokenExpiresAtEpochMillis = tokenExpiresAt,
)

internal const val KEY_HUB_BASE_URL = "hub_base_url"
internal const val KEY_TENANT_ID = "tenant_id"
internal const val KEY_OIDC_DISCOVERY_URL = "oidc_discovery_url"
internal const val KEY_OIDC_CLIENT_ID = "oidc_client_id"
internal const val KEY_OIDC_SCOPES = "oidc_scopes"
internal const val KEY_OIDC_REDIRECT_URI = "oidc_redirect_uri"
internal const val KEY_ACCESS_TOKEN = "access_token"
internal const val KEY_REFRESH_TOKEN = "refresh_token"
internal const val KEY_TOKEN_EXPIRES_AT = "token_expires_at"

internal fun scopesToList(scopes: String?): List<String> =
    scopes?.split(',')?.map { it.trim() }?.filter { it.isNotEmpty() } ?: emptyList()

internal fun listToScopes(scopes: List<String>): String = scopes.joinToString(",")
