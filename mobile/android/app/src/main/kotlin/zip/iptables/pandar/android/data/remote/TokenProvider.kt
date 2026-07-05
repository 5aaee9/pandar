package zip.iptables.pandar.android.data.remote

interface TokenProvider {
    fun currentToken(): String?
}
