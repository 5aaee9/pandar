package zip.iptables.pandar.android.data.remote

import kotlinx.serialization.json.Json

val appJson: Json = Json {
    ignoreUnknownKeys = true
    isLenient = true
    encodeDefaults = true
    explicitNulls = false
}
