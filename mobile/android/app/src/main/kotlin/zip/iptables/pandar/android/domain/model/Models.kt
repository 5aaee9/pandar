package zip.iptables.pandar.android.domain.model

data class Printer(
    val id: String,
    val tenantId: String,
    val agentId: String,
    val serialNumber: String,
    val name: String,
    val model: String?,
    val status: String,
    val lastSeenAt: String,
    val createdAt: String,
    val nozzleTemperatures: List<PrinterNozzleTemp>,
    val activeNozzle: String?,
    val bedTemperatureCelsius: String?,
    val bedTargetTemperatureCelsius: String?,
    val chamberTemperatureCelsius: String?,
    val chamberLightOn: Boolean?,
    val materials: Materials?,
)

data class PrinterNozzleTemp(
    val label: String?,
    val currentCelsius: String?,
    val targetCelsius: String?,
)

data class Materials(
    val amsUnits: List<AmsUnit>,
    val externalSpools: List<ExternalSpool>,
    val activeTray: ActiveTray?,
    val observedAt: String,
)

data class AmsUnit(
    val unitId: String?,
    val humidity: String?,
    val trays: List<AmsTray>,
)

data class AmsTray(
    val trayId: String?,
    val type: String?,
    val color: String?,
    val name: String?,
    val remainingEstimate: String?,
    val kValue: String?,
    val globalTrayId: Int?,
    val exists: Boolean?,
)

data class ExternalSpool(
    val externalId: String?,
    val trayId: String?,
    val type: String?,
    val color: String?,
    val name: String?,
    val remainingEstimate: String?,
    val kValue: String?,
    val globalTrayId: Int?,
    val exists: Boolean?,
)

data class ActiveTray(
    val kind: String?,
    val amsId: String?,
    val trayId: String?,
    val globalTrayId: Int?,
    val externalId: String?,
)

data class Agent(
    val id: String,
    val tenantId: String,
    val name: String,
    val status: String,
    val createdAt: String,
)

data class Command(
    val id: String,
    val tenantId: String,
    val agentId: String,
    val printerId: String?,
    val kind: String,
    val status: String,
    val payloadJson: String,
    val error: String?,
    val resultJson: String?,
    val createdAt: String,
    val updatedAt: String,
)

data class Job(
    val id: String,
    val printerId: String,
    val agentId: String,
    val artifactId: String,
    val commandId: String,
    val status: String,
    val error: String?,
    val createdAt: String,
    val updatedAt: String,
    val print: JobPrint,
    val artifact: JobArtifact,
)

data class JobPrint(
    val status: String,
    val progressPercent: Int?,
    val remainingTimeMinutes: Int?,
    val currentLayer: Int?,
    val totalLayers: Int?,
    val activeFile: String?,
    val error: String?,
    val startedAt: String?,
    val finishedAt: String?,
    val updatedAt: String?,
)

data class JobArtifact(
    val id: String,
    val filename: String,
    val contentType: String,
    val sizeBytes: Long,
    val createdAt: String,
)
