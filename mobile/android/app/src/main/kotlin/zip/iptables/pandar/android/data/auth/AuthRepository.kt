package zip.iptables.pandar.android.data.auth

import android.content.Intent
import android.net.Uri
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import retrofit2.HttpException
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.PandarApi
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeRequest
import zip.iptables.pandar.android.data.settings.SettingsRepository
import java.time.Instant

class AuthRepository(
    private val settings: SettingsRepository,
    private val apiProvider: () -> PandarApi?,
    private val scope: CoroutineScope,
    private val logger: Logger,
) {
    private val _state = MutableStateFlow(AuthState.SIGNED_OUT)
    val state: StateFlow<AuthState> = _state.asStateFlow()

    private val _identity = MutableStateFlow<JwtIdentity?>(null)
    val identity: StateFlow<JwtIdentity?> = _identity.asStateFlow()

    private val _authErrors = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val authErrors: SharedFlow<String> = _authErrors.asSharedFlow()

    init {
        scope.launch {
            settings.settings.collect { snapshot ->
                _identity.value = decodeJwtIdentity(snapshot.accessToken)
                _state.value = when {
                    snapshot.accessToken != null -> AuthState.SIGNED_IN
                    snapshot.hasHubConfig -> AuthState.SIGNED_OUT
                    else -> AuthState.NEEDS_CONFIG
                }
            }
        }
    }

    suspend fun signIn(): AuthEvent {
        val snapshot = currentSettings()
        val hubBaseUrl = snapshot.hubBaseUrl
        if (hubBaseUrl.isNullOrBlank()) {
            return AuthEvent.Toast("Hub URL is not configured.")
        }
        val signInUrl = mobileSignInUrl(hubBaseUrl)
            ?: return AuthEvent.Toast("Hub URL is invalid.")
        _state.value = AuthState.SIGNING_IN
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(signInUrl))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return AuthEvent.LaunchBrowser(intent)
    }

    suspend fun handleAuthorizationResponse(intent: Intent) {
        val data = intent.data ?: return
        if (data.scheme != CALLBACK_SCHEME || data.path != CALLBACK_PATH) {
            return
        }
        val ticket = data.getQueryParameter("ticket")?.takeIf { it.isNotBlank() }
        if (ticket == null) {
            _state.value = AuthState.SIGNED_OUT
            _authErrors.emit("Sign-in failed: no login ticket returned.")
            return
        }

        val api = apiProvider()
        if (api == null) {
            _state.value = AuthState.SIGNED_OUT
            _authErrors.emit("Sign-in failed: Hub URL is not configured.")
            return
        }

        val response = try {
            api.exchangeMobileLoginTicket(MobileTicketExchangeRequest(ticket))
        } catch (t: Throwable) {
            logger.w(t) { "Mobile login ticket exchange failed" }
            _state.value = AuthState.SIGNED_OUT
            _authErrors.emit("Sign-in failed: ${ticketExchangeMessage(t)}.")
            return
        }
        settings.setSession(
            tenantId = response.profile.tenantId,
            access = response.token,
            expiresAtMillis = Instant.parse(response.expiresAt).toEpochMilli(),
        )
    }

    fun refresh(): Boolean = false

    fun endSessionUrl(): String? = null

    fun signOut() {
        scope.launch { settings.clearTokens() }
        _identity.value = null
        _state.value = AuthState.SIGNED_OUT
    }

    private suspend fun currentSettings(): zip.iptables.pandar.android.data.settings.SettingsSnapshot =
        settings.settings.first()

    private fun mobileSignInUrl(hubBaseUrl: String): String? {
        val base = hubBaseUrl.trim().trimEnd('/').toHttpUrlOrNull() ?: return null
        return base.newBuilder()
            .encodedPath("/mobile-sign-in")
            .setQueryParameter("redirect_url", DEFAULT_REDIRECT_URI)
            .build()
            .toString()
    }

    private fun ticketExchangeMessage(error: Throwable): String =
        when (error) {
            is HttpException -> "ticket exchange returned HTTP ${error.code()}"
            else -> error.message ?: "ticket exchange failed"
        }

    companion object {
        const val CALLBACK_SCHEME = "zip.iptables.pandar.android"
        const val CALLBACK_PATH = "/auth/callback"
        const val DEFAULT_REDIRECT_URI = "$CALLBACK_SCHEME:$CALLBACK_PATH"
    }
}
