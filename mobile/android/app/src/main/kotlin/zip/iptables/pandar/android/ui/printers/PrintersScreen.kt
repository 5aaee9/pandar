package zip.iptables.pandar.android.ui.printers

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Print
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import zip.iptables.pandar.android.data.remote.ws.LiveState

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PrintersScreen(
    state: PrintersUiState,
    onOpenPrinter: (String) -> Unit,
    onRefresh: () -> Unit,
) {
    Scaffold { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            SummaryStrip(state)
            HorizontalDivider()
            when {
                state.loading && state.printers.isEmpty() -> Loading()
                state.error != null && state.printers.isEmpty() -> ErrorText(state.error)
                state.printers.isEmpty() -> EmptyFleet()
                else -> PullToRefreshBox(
                    isRefreshing = state.loading,
                    onRefresh = onRefresh,
                ) {
                    LazyColumn(
                        modifier = Modifier.fillMaxSize(),
                        contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        items(state.printers, key = { it.id }) { printer ->
                            PrinterCard(printer = printer, onClick = { onOpenPrinter(printer.id) })
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SummaryStrip(state: PrintersUiState) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        horizontalArrangement = Arrangement.spacedBy(24.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Stat("Printers", "${state.onlinePrinters}/${state.printers.size}")
        Stat("Agents", "${state.connectedAgents}/${state.agents.size}")
        LiveBadge(state.liveState)
    }
}

@Composable
private fun Stat(label: String, value: String) {
    Column {
        Text(label, style = MaterialTheme.typography.labelSmall)
        Text(value, style = MaterialTheme.typography.titleMedium)
    }
}

@Composable
private fun LiveBadge(live: LiveState) {
    val text = when (live) {
        LiveState.CONNECTED -> "Live"
        LiveState.CONNECTING -> "Connecting"
        LiveState.DISCONNECTED -> "Disconnected"
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(Icons.Default.Print, contentDescription = null)
        Spacer(Modifier.padding(4.dp))
        Text(text, style = MaterialTheme.typography.labelSmall)
    }
}

@Composable
private fun Loading() {
    Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        CircularProgressIndicator()
    }
}

@Composable
private fun ErrorText(message: String) {
    Column(Modifier.fillMaxSize().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        Icon(Icons.Default.Build, contentDescription = null)
        Spacer(Modifier.padding(8.dp))
        Text(message, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun EmptyFleet() {
    Column(Modifier.fillMaxSize().padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        Text("No printers", style = MaterialTheme.typography.titleMedium)
        Text("Connect an agent to start monitoring your printers.", style = MaterialTheme.typography.bodySmall)
    }
}
