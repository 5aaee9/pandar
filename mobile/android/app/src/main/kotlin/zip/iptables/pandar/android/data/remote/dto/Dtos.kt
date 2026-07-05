package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

@Serializable
data class PrinterListDto(val printers: List<PrinterDto>)

@Serializable
data class AgentsListDto(val agents: List<AgentDto>)

@Serializable
data class JobListDto(val jobs: List<JobDto>)

@Serializable
data class PrinterDto(
    val id: String,
    @SerialName("tenant_id") val tenantId: String,
    @SerialName("agent_id") val agentId: String,
    @SerialName("serial_number") val serialNumber: String,
    val name: String,
    val model: String? = null,
    val status: String,
    @SerialName("last_seen_at") val lastSeenAt: String,
    @SerialName("created_at") val createdAt: String,
    @SerialName("nozzle_temperatures") val nozzleTemperatures: List<PrinterNozzleTempDto> = emptyList(),
    @SerialName("active_nozzle") val activeNozzle: String? = null,
    @SerialName("bed_temperature_celsius") val bedTemperatureCelsius: String? = null,
    @SerialName("bed_target_temperature_celsius") val bedTargetTemperatureCelsius: String? = null,
    @SerialName("chamber_temperature_celsius") val chamberTemperatureCelsius: String? = null,
    @SerialName("chamber_light_on") val chamberLightOn: Boolean? = null,
    val materials: PrinterMaterialsDto? = null,
)

@Serializable
data class PrinterNozzleTempDto(
    val label: String? = null,
    @SerialName("current_celsius") val currentCelsius: String? = null,
    @SerialName("target_celsius") val targetCelsius: String? = null,
)

@Serializable
data class AgentDto(
    val id: String,
    @SerialName("tenant_id") val tenantId: String,
    val name: String,
    val status: String,
    @SerialName("created_at") val createdAt: String,
)

@Serializable
data class CommandResponseDto(
    val id: String,
    @SerialName("tenant_id") val tenantId: String,
    @SerialName("agent_id") val agentId: String,
    @SerialName("printer_id") val printerId: String? = null,
    val kind: String,
    val status: String,
    @SerialName("payload_json") val payloadJson: String,
    val error: String? = null,
    @SerialName("result_json") val resultJson: String? = null,
    @SerialName("created_at") val createdAt: String,
    @SerialName("updated_at") val updatedAt: String,
)

@Serializable
data class JobDto(
    val id: String,
    @SerialName("printer_id") val printerId: String,
    @SerialName("agent_id") val agentId: String,
    @SerialName("artifact_id") val artifactId: String,
    @SerialName("command_id") val commandId: String,
    val status: String,
    val error: String? = null,
    @SerialName("created_at") val createdAt: String,
    @SerialName("updated_at") val updatedAt: String,
    val print: JobPrintDto,
    val command: JobCommandDto? = null,
    val artifact: JobArtifactDto,
)

@Serializable
data class JobPrintDto(
    val status: String,
    @SerialName("printer_state") val printerState: String? = null,
    @SerialName("progress_percent") val progressPercent: Int? = null,
    @SerialName("remaining_time_minutes") val remainingTimeMinutes: Int? = null,
    @SerialName("current_layer") val currentLayer: Int? = null,
    @SerialName("total_layers") val totalLayers: Int? = null,
    @SerialName("active_file") val activeFile: String? = null,
    @SerialName("last_progress_percent") val lastProgressPercent: Int? = null,
    @SerialName("last_layer") val lastLayer: Int? = null,
    val error: String? = null,
    @SerialName("started_at") val startedAt: String? = null,
    @SerialName("finished_at") val finishedAt: String? = null,
    @SerialName("updated_at") val updatedAt: String? = null,
)

@Serializable
data class JobCommandDto(val id: String, val kind: String, val status: String)

@Serializable
data class JobArtifactDto(
    val id: String,
    @SerialName("tenant_id") val tenantId: String,
    val filename: String,
    @SerialName("content_type") val contentType: String,
    @SerialName("size_bytes") val sizeBytes: Long,
    val metadata: JsonElement? = null,
    @SerialName("created_at") val createdAt: String,
)
