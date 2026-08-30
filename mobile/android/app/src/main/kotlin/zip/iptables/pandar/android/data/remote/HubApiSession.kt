package zip.iptables.pandar.android.data.remote

class HubApiSession(
    val context: HubSessionContext,
    val api: PandarApi,
) {
    val identity: HubSession
        get() = context.identity
}
