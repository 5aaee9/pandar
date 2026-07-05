package zip.iptables.pandar.android.core.util

interface Logger {
    fun d(t: Throwable? = null, msg: () -> String)
    fun w(t: Throwable? = null, msg: () -> String)
    fun e(t: Throwable? = null, msg: () -> String)
}
