package zip.iptables.pandar.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.lifecycle.ViewModelProvider
import zip.iptables.pandar.android.ui.navigation.PandarNavGraph
import zip.iptables.pandar.android.ui.theme.PandarTheme
import zip.iptables.pandar.android.ui.viewmodel.PandarViewModelFactory

class MainActivity : ComponentActivity() {

    private lateinit var mainVm: MainActivityViewModel
    private lateinit var authResultLauncher: ActivityResultLauncher<Intent>

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val container = (application as PandarApplication).container
        mainVm = ViewModelProvider(this, PandarViewModelFactory.create(container))[MainActivityViewModel::class.java]

        authResultLauncher = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val data = result.data
            if (data != null) {
                mainVm.handleAuthorizationResponse(data)
            } else {
                // User cancelled the browser flow; reset the signing-in state so the login gate
                // becomes interactive again instead of staying on the spinner forever.
                mainVm.cancelSignIn()
            }
        }

        setContent {
            PandarTheme {
                Surface(color = MaterialTheme.colorScheme.background) {
                    val browserIntent by mainVm.browserEvents.collectAsState(initial = null)
                    val openUrl by mainVm.openUrl.collectAsState(initial = null)
                    LaunchedEffect(browserIntent) {
                        browserIntent?.let { authResultLauncher.launch(it) }
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
