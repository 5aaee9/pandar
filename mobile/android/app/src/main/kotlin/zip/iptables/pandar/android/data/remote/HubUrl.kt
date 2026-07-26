package zip.iptables.pandar.android.data.remote

import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

fun secureHubHttpUrl(value: String?): HttpUrl? {
    val url = value?.trim()?.trimEnd('/')?.toHttpUrlOrNull() ?: return null
    val loopback = url.host == "localhost" || url.host == "127.0.0.1" || url.host == "::1"
    return url.takeIf { it.isHttps || it.scheme == "http" && loopback }
}
