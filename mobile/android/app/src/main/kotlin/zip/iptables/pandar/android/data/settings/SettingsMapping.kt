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
    accessToken = values[KEY_ACCESS_TOKEN],
    tokenExpiresAtEpochMillis = tokenExpiresAt,
)

internal const val KEY_HUB_BASE_URL = "hub_base_url"
internal const val KEY_TENANT_ID = "tenant_id"
internal const val KEY_ACCESS_TOKEN = "access_token"
internal const val KEY_TOKEN_EXPIRES_AT = "token_expires_at"
