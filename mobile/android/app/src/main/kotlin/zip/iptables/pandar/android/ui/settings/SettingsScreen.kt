package zip.iptables.pandar.android.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.data.auth.AuthState
import zip.iptables.pandar.android.ui.components.LabeledTextField
import zip.iptables.pandar.android.ui.components.PrimaryButton

@Composable
fun SettingsScreen(
    state: SettingsUiState,
    onEdit: ((zip.iptables.pandar.android.data.settings.SettingsSnapshot) -> zip.iptables.pandar.android.data.settings.SettingsSnapshot) -> Unit,
    onSave: () -> Unit,
    onSignIn: () -> Unit,
    onSignOut: () -> Unit,
) {
    Scaffold { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .padding(16.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Connection", style = MaterialTheme.typography.titleMedium)
            LabeledTextField(
                label = "Hub base URL",
                value = state.snapshot.hubBaseUrl.orEmpty(),
                onChange = { v -> onEdit { it.copy(hubBaseUrl = v) } },
                placeholder = "https://hub.example.com/",
            )

            Spacer(Modifier.height(8.dp))
            PrimaryButton(
                text = if (state.saved) "Saved" else "Save",
                onClick = onSave,
                modifier = Modifier.fillMaxWidth(),
            )

            HorizontalDivider()
            Text("Account", style = MaterialTheme.typography.titleMedium)
            state.snapshot.tenantId?.let { tenantId ->
                Text("Tenant: $tenantId", style = MaterialTheme.typography.bodySmall)
            }
            val subject = state.identitySubject
            val issuer = state.identityIssuer
            if (subject != null || issuer != null) {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    subject?.let { Text("Signed in as: $it", style = MaterialTheme.typography.bodySmall) }
                    issuer?.let { Text("Issuer: $it", style = MaterialTheme.typography.labelSmall) }
                }
            }
            when (state.authState) {
                AuthState.SIGNED_IN -> Button(onClick = onSignOut, modifier = Modifier.fillMaxWidth()) {
                    Text("Sign out")
                }
                AuthState.NEEDS_CONFIG -> Text(
                    "Save the Hub URL, then sign in.",
                    style = MaterialTheme.typography.bodySmall,
                )
                else -> Button(onClick = onSignIn, modifier = Modifier.fillMaxWidth()) { Text("Sign in") }
            }
        }
    }
}
