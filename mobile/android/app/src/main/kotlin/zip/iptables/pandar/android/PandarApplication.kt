package zip.iptables.pandar.android

import android.app.Application
import zip.iptables.pandar.android.core.di.AppContainer

class PandarApplication : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        container.startLiveUpdates()
    }
}
