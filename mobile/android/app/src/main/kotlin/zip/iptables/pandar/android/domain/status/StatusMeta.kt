package zip.iptables.pandar.android.domain.status

import zip.iptables.pandar.android.domain.model.Severity

data class StatusMeta(val severity: Severity, val label: String)

private val SUCCESS_TOKENS = setOf(
    "online", "ok", "succeeded", "completed", "running", "printing", "ready",
)
private val WARNING_TOKENS = setOf(
    "warning", "queued", "sent", "acknowledged", "connecting", "problem", "degraded", "pending",
)
private val CRITICAL_TOKENS = setOf(
    "failed", "offline", "unavailable", "error", "down",
)

fun statusMeta(rawStatus: String): StatusMeta {
    val normalized = rawStatus.trim().lowercase()
    val severity = when {
        normalized.isEmpty() -> Severity.INFO
        SUCCESS_TOKENS.contains(normalized) -> Severity.SUCCESS
        WARNING_TOKENS.contains(normalized) -> Severity.WARNING
        CRITICAL_TOKENS.contains(normalized) -> Severity.CRITICAL
        else -> Severity.INFO
    }
    return StatusMeta(severity, prettifyLabel(rawStatus))
}

private fun prettifyLabel(raw: String): String {
    val cleaned = raw.trim().replace(Regex("[_\\-]+"), " ").trim()
    if (cleaned.isEmpty()) return "Unknown"
    return cleaned[0].uppercaseChar() + cleaned.substring(1)
}
