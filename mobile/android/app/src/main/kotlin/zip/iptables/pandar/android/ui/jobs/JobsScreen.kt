package zip.iptables.pandar.android.ui.jobs

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.ui.components.StatusPill

@Composable
fun JobsScreen(
    state: JobsUiState,
    onRetry: (String) -> Unit,
    onReprint: (String) -> Unit,
) {
    Scaffold { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            if (state.jobs.isEmpty()) {
                Empty(state)
                return@Column
            }
            state.toast?.let {
                Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(16.dp))
            }
            LazyColumn(
                contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(state.jobs, key = { it.id }) { job ->
                    JobRow(job, isInFlight = state.inFlightId == job.id, onRetry, onReprint)
                }
            }
        }
    }
}

@Composable
private fun JobRow(job: Job, isInFlight: Boolean, onRetry: (String) -> Unit, onReprint: (String) -> Unit) {
    val successStatuses = setOf("completed", "succeeded")
    val isSuccessful = job.status.lowercase() in successStatuses || job.print.status.lowercase() in successStatuses
    // Retry only makes sense for a job that did not succeed; Reprint is always available.
    val retryEnabled = !isInFlight && !isSuccessful
    val reprintEnabled = !isInFlight
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                Text(job.artifact.filename, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f), fontFamily = FontFamily.Monospace)
                StatusPill(job.print.status)
            }
            Text("Job ${job.status}", style = MaterialTheme.typography.bodySmall)
            Text("Created ${job.createdAt} · Updated ${job.updatedAt}", style = MaterialTheme.typography.labelSmall)
            ProgressRow(job)
            HorizontalDivider()
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { onRetry(job.id) }, enabled = retryEnabled) { Text("Retry") }
                OutlinedButton(onClick = { onReprint(job.id) }, enabled = reprintEnabled) { Text("Reprint") }
            }
        }
    }
}

@Composable
private fun ProgressRow(job: Job) {
    val percent = job.print.progressPercent
    val remaining = job.print.remainingTimeMinutes
    val layers = listOfNotNull(job.print.currentLayer, job.print.totalLayers)
        .joinToString("/")
    val text = buildString {
        if (percent != null) append("$percent%")
        if (remaining != null) {
            if (isNotEmpty()) append(" · ")
            append(formatRemaining(remaining))
        }
        if (layers.isNotEmpty()) {
            if (isNotEmpty()) append(" · ")
            append("layer $layers")
        }
    }
    if (text.isNotEmpty()) Text(text, style = MaterialTheme.typography.bodyMedium)
}

@Composable
private fun Empty(state: JobsUiState) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = androidx.compose.ui.Alignment.CenterHorizontally,
    ) {
        Text(state.error ?: "No jobs", style = MaterialTheme.typography.titleMedium)
        Text("Dispatch a print from the web dashboard to see it here.", style = MaterialTheme.typography.bodySmall)
    }
}

private fun formatRemaining(minutes: Int): String =
    if (minutes < 60) "${minutes}m" else "${minutes / 60}h ${minutes % 60}m"
