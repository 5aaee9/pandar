package zip.iptables.pandar.android.domain.model

data class PandarState(
    val sessionGeneration: Long = 0,
    val hasSession: Boolean = false,
    val printers: List<Printer> = emptyList(),
    val jobs: List<Job> = emptyList(),
    val latestCommandsByPrinter: Map<String, Command> = emptyMap(),
)
