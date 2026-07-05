package zip.iptables.pandar.android.data.auth

sealed interface AuthEvent {
    data class LaunchBrowser(val intent: android.content.Intent) : AuthEvent
    data class Toast(val message: String) : AuthEvent
}
