package zip.iptables.pandar.android.ui.login

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.data.auth.AuthEvent
import zip.iptables.pandar.android.data.auth.AuthState

@Composable
fun LoginScreen(
    state: AuthState,
    onSignIn: () -> Unit,
    onLaunchBrowser: (android.content.Intent) -> Unit,
    onToast: (String) -> Unit,
    errorMessage: String? = null,
) {
    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("Pandar", style = MaterialTheme.typography.headlineMedium)
            Spacer(Modifier.height(8.dp))
            Text(
                "Sign in with your configured identity provider to monitor and control your printers.",
                style = MaterialTheme.typography.bodyMedium,
            )
            Spacer(Modifier.height(16.dp))
            errorMessage?.let {
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                Spacer(Modifier.height(8.dp))
            }
            when (state) {
                AuthState.SIGNING_IN -> CircularProgressIndicator()
                AuthState.NEEDS_CONFIG -> Text(
                    "Configure the hub and OIDC provider in Settings first.",
                    style = MaterialTheme.typography.bodySmall,
                )
                else -> Button(onClick = onSignIn) { Text("Sign in") }
            }
        }
    }
}
