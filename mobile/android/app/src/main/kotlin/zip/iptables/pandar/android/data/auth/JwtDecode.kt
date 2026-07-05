package zip.iptables.pandar.android.data.auth

import android.util.Base64

/** Best-effort decode of a JWT's payload claims (sub, iss, email, name). Never throws. */
data class JwtIdentity(val subject: String?, val issuer: String?, val email: String?, val name: String?)

fun decodeJwtIdentity(token: String?): JwtIdentity? {
    if (token.isNullOrEmpty()) return null
    val parts = token.split(".")
    if (parts.size < 2) return null
    return try {
        val payload = String(Base64.decode(parts[1], Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP))
        val json = org.json.JSONObject(payload)
        JwtIdentity(
            subject = json.optString("sub").takeIf { it.isNotEmpty() },
            issuer = json.optString("iss").takeIf { it.isNotEmpty() },
            email = json.optString("email").takeIf { it.isNotEmpty() },
            name = json.optString("name").takeIf { it.isNotEmpty() },
        )
    } catch (_: Throwable) {
        null
    }
}
