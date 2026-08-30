package zip.iptables.pandar.android.data.settings

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.data.remote.HubSession
import zip.iptables.pandar.android.data.remote.TokenProvider

private val Context.pandarDataStore by preferencesDataStore(name = "pandar_settings")

class SettingsRepository(
    private val context: Context,
    private val scope: CoroutineScope,
) : TokenProvider {

    private val stringKeys = mapOf(
        KEY_HUB_BASE_URL to stringPreferencesKey(KEY_HUB_BASE_URL),
        KEY_TENANT_ID to stringPreferencesKey(KEY_TENANT_ID),
        KEY_ACCESS_TOKEN to stringPreferencesKey(KEY_ACCESS_TOKEN),
    )
    private val expiresAtKey = longPreferencesKey(KEY_TOKEN_EXPIRES_AT)

    val settings: Flow<SettingsSnapshot> = context.pandarDataStore.data.map { prefs ->
        settingsToSnapshot(
            values = stringKeys.mapValues { (_, key) -> prefs[key] },
            tokenExpiresAt = prefs[expiresAtKey],
        )
    }

    val tenantId: Flow<String?> = settings.map { it.tenantId }

    suspend fun update(transform: (SettingsSnapshot) -> SettingsSnapshot) {
        context.pandarDataStore.edit { prefs ->
            val current = settingsToSnapshot(
                values = stringKeys.mapValues { (_, key) -> prefs[key] },
                tokenExpiresAt = prefs[expiresAtKey],
            )
            val updated = transform(current)
            val hubChanged = updated.hubBaseUrl != current.hubBaseUrl
            prefs.putOrRemove(stringKeys.getValue(KEY_HUB_BASE_URL), updated.hubBaseUrl)
            prefs.putOrRemove(
                stringKeys.getValue(KEY_TENANT_ID),
                if (hubChanged) null else updated.tenantId,
            )
            prefs.putOrRemove(
                stringKeys.getValue(KEY_ACCESS_TOKEN),
                if (hubChanged) null else updated.accessToken,
            )
            if (!hubChanged && updated.tokenExpiresAtEpochMillis != null) {
                prefs[expiresAtKey] = updated.tokenExpiresAtEpochMillis
            } else {
                prefs.remove(expiresAtKey)
            }
        }
    }

    suspend fun setSession(tenantId: String, access: String, expiresAtMillis: Long?) {
        update {
            it.copy(
                tenantId = tenantId,
                accessToken = access,
                tokenExpiresAtEpochMillis = expiresAtMillis,
            )
        }
    }

    suspend fun clearTokens() {
        update { it.copy(accessToken = null, tokenExpiresAtEpochMillis = null) }
    }

    suspend fun clearSessionIfCurrent(expected: HubSession) {
        update { current -> current.clearSessionIfMatches(expected) }
    }

    // Best-effort cache of the latest snapshot for synchronous token access from the
    // network interceptor. The OkHttp interceptor runs on non-suspend threads, so it
    // cannot read the Flow directly.
    @Volatile
    private var lastSnapshot: SettingsSnapshot? = null

    init {
        scope.launch {
            settings.collect { lastSnapshot = it }
        }
    }

    override fun currentToken(): String? = lastSnapshot?.accessToken

    fun currentTenant(): String? = lastSnapshot?.tenantId
}

private fun androidx.datastore.preferences.core.MutablePreferences.putOrRemove(
    key: androidx.datastore.preferences.core.Preferences.Key<String>,
    value: String?,
) {
    if (value == null) {
        remove(key)
    } else {
        this[key] = value
    }
}

