package zip.iptables.pandar.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.lifecycle.ViewModelProvider
import zip.iptables.pandar.android.ui.navigation.PandarNavGraph
import zip.iptables.pandar.android.ui.theme.PandarTheme
import zip.iptables.pandar.android.ui.viewmodel.PandarViewModelFactory

class MainActivity : ComponentActivity() {

    private lateinit var mainVm: MainActivityViewModel

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val container = (application as PandarApplication).container
        mainVm = ViewModelProvider(this, PandarViewModelFactory.create(container))[MainActivityViewModel::class.java]
        if (intent?.action == Intent.ACTION_VIEW) {
            mainVm.handleAuthorizationResponse(intent)
        }

        setContent {
            PandarTheme {
                Surface(color = MaterialTheme.colorScheme.background) {
                    val browserIntent by mainVm.browserEvents.collectAsState(initial = null)
                    val openUrl by mainVm.openUrl.collectAsState(initial = null)
                    LaunchedEffect(browserIntent) {
                        browserIntent?.let { startActivity(it) }
                    }
                    LaunchedEffect(openUrl) {
                        openUrl?.let { url ->
                            val intent = Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url))
                                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                            startActivity(intent)
                        }
                    }
                    PandarNavGraph(mainVm = mainVm)
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        mainVm.handleAuthorizationResponse(intent)
    }
}
