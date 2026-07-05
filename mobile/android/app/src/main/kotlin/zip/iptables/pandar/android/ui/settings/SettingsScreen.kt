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
            LabeledTextField(
                label = "Tenant ID",
                value = state.snapshot.tenantId.orEmpty(),
                onChange = { v -> onEdit { it.copy(tenantId = v) } },
                placeholder = "00000000-0000-0000-0000-000000000000",
            )

            HorizontalDivider()
            Text("OIDC provider", style = MaterialTheme.typography.titleMedium)
            LabeledTextField(
                label = "Discovery URL",
                value = state.snapshot.oidcDiscoveryUrl.orEmpty(),
                onChange = { v -> onEdit { it.copy(oidcDiscoveryUrl = v) } },
                placeholder = "https://idp.example/.well-known/openid-configuration",
            )
            LabeledTextField(
                label = "Client ID",
                value = state.snapshot.oidcClientId.orEmpty(),
                onChange = { v -> onEdit { it.copy(oidcClientId = v) } },
            )
            LabeledTextField(
                label = "Scopes (comma-separated)",
                value = state.snapshot.oidcScopes.orEmpty(),
                onChange = { v -> onEdit { it.copy(oidcScopes = v) } },
                placeholder = "openid,profile",
            )
            LabeledTextField(
                label = "Redirect URI",
                value = state.snapshot.oidcRedirectUri.orEmpty(),
                onChange = { v -> onEdit { it.copy(oidcRedirectUri = v) } },
                placeholder = "zip.iptables.pandar.android:/oauth2redirect",
            )

            Spacer(Modifier.height(8.dp))
            PrimaryButton(
                text = if (state.saved) "Saved" else "Save",
                onClick = onSave,
                modifier = Modifier.fillMaxWidth(),
            )

            HorizontalDivider()
            Text("Account", style = MaterialTheme.typography.titleMedium)
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
                    "Save a discovery URL and client ID, then sign in.",
                    style = MaterialTheme.typography.bodySmall,
                )
                else -> Button(onClick = onSignIn, modifier = Modifier.fillMaxWidth()) { Text("Sign in") }
            }
        }
    }
}
