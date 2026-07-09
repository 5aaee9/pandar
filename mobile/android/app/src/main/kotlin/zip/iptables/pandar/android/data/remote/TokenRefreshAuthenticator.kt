package zip.iptables.pandar.android.data.remote

import kotlinx.coroutines.runBlocking
import okhttp3.Authenticator
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route
import zip.iptables.pandar.android.core.util.Logger

/**
 * On a 401 response, attempts a single token refresh and retries the request with the new token.
 * Returns null (giving up) if refresh fails or the request has already been retried, so the caller
 * surfaces the 401 to the user. Refresh runs via the injected suspending [refresher].
 */
class TokenRefreshAuthenticator(
    private val tokenProvider: TokenProvider,
    private val refresher: suspend () -> Boolean,
    private val clearTokens: () -> Unit,
    private val logger: Logger,
) : Authenticator {

    override fun authenticate(route: Route?, response: Response): Request? {
        if (response.code != 401) return null
        // Avoid infinite retry loops: only retry once.
        if (responseCount(response) >= 2) return null

        val refreshed = try {
            runBlocking { refresher() }
        } catch (t: Throwable) {
            logger.w(t) { "Token refresh on 401 threw" }
            false
        }
        if (!refreshed) {
            // Refresh failed: discard tokens so AuthState flips to SIGNED_OUT and the
            // sign-in gate reappears.
            clearTokens()
            return null
        }

        val newToken = tokenProvider.currentToken() ?: return null
        return response.request.newBuilder()
            .header("Authorization", "Bearer $newToken")
            .build()
    }

    private fun responseCount(response: Response): Int {
        var current: Response? = response
        var count = 1
        while (current?.priorResponse != null) {
            count++
            current = current.priorResponse
        }
        return count
    }
}
