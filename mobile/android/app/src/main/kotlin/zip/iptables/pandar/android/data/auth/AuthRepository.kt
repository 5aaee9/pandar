package zip.iptables.pandar.android.data.auth

import android.content.Intent
import android.net.Uri
import android.util.Base64
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

import retrofit2.HttpException
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.remote.PandarApi
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeRequest
import zip.iptables.pandar.android.data.remote.secureHubHttpUrl
import zip.iptables.pandar.android.data.settings.SettingsRepository
import java.time.Instant
import java.security.MessageDigest
import java.security.SecureRandom

class AuthRepository(
    private val settings: SettingsRepository,
    private val apiProvider: () -> PandarApi?,
    private val scope: CoroutineScope,
    private val logger: Logger,
) {
    private data class PendingAuthorization(val state: String, val codeVerifier: String)

    private var pendingAuthorization: PendingAuthorization? = null
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
        val authorization = PendingAuthorization(randomBase64Url(), randomBase64Url())
        val signInUrl = mobileSignInUrl(hubBaseUrl, authorization)
            ?: return AuthEvent.Toast("Hub URL is invalid.")
        pendingAuthorization = authorization
        _state.value = AuthState.SIGNING_IN
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(signInUrl))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return AuthEvent.LaunchBrowser(intent)
    }

    suspend fun handleAuthorizationResponse(intent: Intent) {
        val data = intent.data ?: return
        if (
            data.scheme != CALLBACK_SCHEME ||
            data.host != CALLBACK_HOST ||
            data.path != CALLBACK_PATH
        ) {
            return
        }
        val authorization = pendingAuthorization ?: return
        val state = data.getQueryParameter("state") ?: return
        if (!MessageDigest.isEqual(state.toByteArray(), authorization.state.toByteArray())) {
            return
        }
        val ticket = data.getQueryParameter("ticket")?.takeIf { it.isNotBlank() }
        if (ticket == null) {
            pendingAuthorization = null
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
            api.exchangeMobileLoginTicket(
                MobileTicketExchangeRequest(ticket, authorization.codeVerifier),
            )
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
        pendingAuthorization = null
    }

    fun refresh(): Boolean = false

    fun endSessionUrl(): String? = null

    suspend fun signOut() {
        pendingAuthorization = null
        settings.clearTokens()
    }

    private suspend fun currentSettings(): zip.iptables.pandar.android.data.settings.SettingsSnapshot =
        settings.settings.first()

    private fun mobileSignInUrl(
        hubBaseUrl: String,
        authorization: PendingAuthorization,
    ): String? {
        val base = secureHubHttpUrl(hubBaseUrl) ?: return null
        return base.newBuilder()
            .encodedPath("/mobile-sign-in")
            .setQueryParameter("redirect_url", DEFAULT_REDIRECT_URI)
            .setQueryParameter("state", authorization.state)
            .setQueryParameter("code_challenge", pkceChallenge(authorization.codeVerifier))
            .build()
            .toString()
    }

    private fun randomBase64Url(): String {
        val bytes = ByteArray(32)
        SecureRandom().nextBytes(bytes)
        return Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
    }

    private fun pkceChallenge(verifier: String): String = Base64.encodeToString(
        MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray()),
        Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING,
    )

    private fun ticketExchangeMessage(error: Throwable): String =
        when (error) {
            is HttpException -> "ticket exchange returned HTTP ${error.code()}"
            else -> error.message ?: "ticket exchange failed"
        }

    companion object {
        const val CALLBACK_SCHEME = "zip.iptables.pandar.android"
        const val CALLBACK_HOST = "auth"
        const val CALLBACK_PATH = "/callback"
        const val DEFAULT_REDIRECT_URI = "$CALLBACK_SCHEME://$CALLBACK_HOST$CALLBACK_PATH"
    }
}
