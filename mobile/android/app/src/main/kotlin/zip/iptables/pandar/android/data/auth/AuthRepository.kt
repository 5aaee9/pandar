package zip.iptables.pandar.android.data.auth

import android.content.Context
import android.content.Intent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import net.openid.appauth.AuthorizationException
import net.openid.appauth.AuthorizationRequest
import net.openid.appauth.AuthorizationResponse
import net.openid.appauth.AuthorizationService
import net.openid.appauth.AuthorizationServiceConfiguration
import net.openid.appauth.CodeVerifierUtil
import net.openid.appauth.GrantTypeValues
import net.openid.appauth.ResponseTypeValues
import net.openid.appauth.TokenRequest
import net.openid.appauth.TokenResponse
import net.openid.appauth.connectivity.ConnectionBuilder
import zip.iptables.pandar.android.core.util.Logger
import zip.iptables.pandar.android.data.settings.SettingsRepository
import zip.iptables.pandar.android.data.settings.scopesToList
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume

class AuthRepository(
    private val context: Context,
    private val settings: SettingsRepository,
    private val scope: CoroutineScope,
    private val logger: Logger,
) {
    private val _state = MutableStateFlow(AuthState.SIGNED_OUT)
    val state: StateFlow<AuthState> = _state.asStateFlow()

    /** Best-effort identity decoded from the current access-token JWT (null if unavailable). */
    private val _identity = MutableStateFlow<JwtIdentity?>(null)
    val identity: StateFlow<JwtIdentity?> = _identity.asStateFlow()

    /** One-shot auth error messages (e.g. redirect/token-exchange failures) for UI surfacing. */
    private val _authErrors = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val authErrors: SharedFlow<String> = _authErrors.asSharedFlow()

    /** Last seen AppAuth configuration, retained so sign-out can use the end-session endpoint. */
    @Volatile
    private var lastEndSessionEndpoint: String? = null

    private val pendingRequest = AtomicReference<AuthorizationRequest?>(null)

    init {
        scope.launch {
            settings.settings.collect { snapshot ->
                _identity.value = decodeJwtIdentity(snapshot.accessToken)
                _state.value = when {
                    // Authenticated via OIDC.
                    snapshot.accessToken != null -> AuthState.SIGNED_IN
                    // OIDC configured but no token yet → user must sign in.
                    snapshot.hasOidcConfig -> AuthState.SIGNED_OUT
                    // Hub + tenant configured without OIDC → no-auth hub, operate without a token.
                    snapshot.hasHubConfig -> AuthState.SIGNED_IN
                    // Nothing configured yet.
                    else -> AuthState.NEEDS_CONFIG
                }
            }
        }
    }

    /**
     * Begins the OIDC Authorization Code + PKCE flow. Returns a [AuthEvent] describing either a
     * browser launch or an error toast. The caller must forward the redirect Intent to
     * [handleAuthorizationResponse].
     */
    suspend fun signIn(): AuthEvent {
        val snapshot = currentSettings()
        val discoveryUrl = snapshot.oidcDiscoveryUrl
        val clientId = snapshot.oidcClientId
        if (discoveryUrl.isNullOrEmpty() || clientId.isNullOrEmpty()) {
            return AuthEvent.Toast("OIDC is not configured.")
        }
        val redirectUri = (snapshot.oidcRedirectUri?.takeIf { it.isNotEmpty() })
            ?: DEFAULT_REDIRECT_URI

        val config = try {
            fetchConfiguration(discoveryUrl)
        } catch (t: Throwable) {
            logger.w(t) { "OIDC discovery failed" }
            return AuthEvent.Toast("OIDC discovery failed: ${t.message}")
        }
        lastEndSessionEndpoint = config.endSessionEndpoint?.toString()

        val requestBuilder = AuthorizationRequest.Builder(
            config,
            clientId,
            ResponseTypeValues.CODE,
            android.net.Uri.parse(redirectUri),
        )
        requestBuilder.setCodeVerifier(CodeVerifierUtil.generateRandomCodeVerifier())
        requestBuilder.setScopes(buildList {
            add("openid")
            addAll(scopesToList(snapshot.oidcScopes))
        })

        val request = requestBuilder.build()
        pendingRequest.set(request)

        val service = AuthorizationService(context)
        val intent: Intent = try {
            service.getAuthorizationRequestIntent(request)
        } catch (t: Throwable) {
            logger.w(t) { "Failed to build authorization intent" }
            return AuthEvent.Toast("Could not start sign-in: ${t.message}")
        }

        _state.value = AuthState.SIGNING_IN
        return AuthEvent.LaunchBrowser(intent)
    }

    /**
     * Resumes the flow after the browser redirect. Exchanges the authorization code for tokens
     * using the PKCE verifier from the original request.
     */
    suspend fun handleAuthorizationResponse(intent: Intent) {
        val response = AuthorizationResponse.fromIntent(intent) ?: run {
            _state.value = AuthState.SIGNED_OUT
            _authErrors.tryEmit("Sign-in failed: no authorization response.")
            return
        }
        val exception = AuthorizationException.fromIntent(intent)
        if (exception != null) {
            logger.w(exception) { "Authorization error" }
            _state.value = AuthState.SIGNED_OUT
            _authErrors.tryEmit("Sign-in failed: ${exception.errorDescription ?: exception.error ?: "authorization error"}.")
            return
        }

        val request = pendingRequest.get() ?: response.request
        val tokenExchangeRequest = response.createTokenExchangeRequest()
        val tokenResponse = performTokenRequest(tokenExchangeRequest)
        if (tokenResponse == null) {
            _state.value = AuthState.SIGNED_OUT
            _authErrors.tryEmit("Sign-in failed: token exchange returned no response.")
            return
        }
        if (tokenResponse.accessToken.isNullOrEmpty()) {
            _state.value = AuthState.SIGNED_OUT
            _authErrors.tryEmit("Sign-in failed: token response had no access token.")
            return
        }

        settings.setTokens(
            access = tokenResponse.accessToken,
            refresh = tokenResponse.refreshToken,
            expiresAtMillis = tokenResponse.accessTokenExpirationTime,
        )
        pendingRequest.set(null)
    }

    /** Attempts a refresh-token grant; returns true on success. */
    suspend fun refresh(): Boolean {
        val snapshot = currentSettings()
        val refreshToken = snapshot.refreshToken ?: return false
        val discoveryUrl = snapshot.oidcDiscoveryUrl ?: return false
        val clientId = snapshot.oidcClientId ?: return false
        val config = try {
            fetchConfiguration(discoveryUrl)
        } catch (t: Throwable) {
            logger.w(t) { "Refresh failed: discovery error" }
            return false
        }
        val tokenRequest = TokenRequest.Builder(config, clientId)
            .setGrantType(GrantTypeValues.REFRESH_TOKEN)
            .setRefreshToken(refreshToken)
            .build()
        val tokenResponse = performTokenRequest(tokenRequest) ?: return false
        settings.setTokens(
            access = tokenResponse.accessToken ?: snapshot.accessToken,
            refresh = tokenResponse.refreshToken ?: refreshToken,
            expiresAtMillis = tokenResponse.accessTokenExpirationTime,
        )
        return true
    }

    fun endSessionUrl(): String? = lastEndSessionEndpoint

    fun signOut() {
        scope.launch { settings.clearTokens() }
        pendingRequest.set(null)
        _identity.value = null
        _state.value = AuthState.SIGNED_OUT
    }

    private suspend fun currentSettings(): zip.iptables.pandar.android.data.settings.SettingsSnapshot =
        settings.settings.first()

    private suspend fun fetchConfiguration(discoveryUrl: String): AuthorizationServiceConfiguration =
        suspendCancellableCoroutine { cont ->
            AuthorizationServiceConfiguration.fetchFromUrl(
                android.net.Uri.parse(discoveryUrl),
                object : AuthorizationServiceConfiguration.RetrieveConfigurationCallback {
                    override fun onFetchConfigurationReturned(
                        serviceConfig: AuthorizationServiceConfiguration?,
                        ex: AuthorizationException?,
                    ) {
                        if (serviceConfig != null) {
                            cont.resume(serviceConfig)
                        } else {
                            cont.resumeWith(Result.failure(ex ?: IllegalStateException("discovery failed")))
                        }
                    }
                },
                AppAuthConnectionBuilder,
            )
        }

    private suspend fun performTokenRequest(request: TokenRequest): TokenResponse? =
        suspendCancellableCoroutine { cont ->
            val service = AuthorizationService(context)
            service.performTokenRequest(request) { response, ex ->
                if (response != null) {
                    cont.resume(response)
                } else {
                    val message = ex?.errorDescription ?: ex?.error ?: ex?.message ?: "token exchange failed"
                    logger.w(ex) { "Token exchange failed: $message" }
                    scope.launch { _authErrors.emit("Sign-in failed: $message.") }
                    cont.resume(null)
                }
            }
        }

    companion object {
        const val DEFAULT_REDIRECT_URI = "zip.iptables.pandar.android:/oauth2redirect"
    }
}

/**
 * Default AppAuth connection builder that opens HttpURLConnections. AppAuth's built-in
 * DefaultConnectionBuilder is package-private, so we provide a minimal equivalent.
 */
private object AppAuthConnectionBuilder : ConnectionBuilder {
    override fun openConnection(uri: android.net.Uri): HttpURLConnection {
        val url = URL(uri.toString())
        val conn = url.openConnection() as HttpURLConnection
        conn.connectTimeout = 15_000
        conn.readTimeout = 15_000
        conn.instanceFollowRedirects = true
        return conn
    }
}
