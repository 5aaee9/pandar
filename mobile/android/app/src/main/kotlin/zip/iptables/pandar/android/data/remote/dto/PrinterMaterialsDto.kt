package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonClassDiscriminator
import kotlinx.serialization.json.JsonElement

@Serializable
data class PrinterMaterialsDto(
    @SerialName("ams_units") val amsUnits: JsonElement? = null,
    @SerialName("external_spools") val externalSpools: JsonElement? = null,
    @SerialName("active_tray") val activeTray: JsonElement? = null,
    @SerialName("observed_at") val observedAt: String = "",
)

@Serializable
data class PrinterEventCommandDto(
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
@JsonClassDiscriminator("type")
sealed class PrinterEventDto {
    @Serializable
    @SerialName("printer_snapshot")
    data class PrinterSnapshot(val printer: PrinterDto) : PrinterEventDto()

    @Serializable
    @SerialName("job_progress")
    data class JobProgress(val job: JobDto) : PrinterEventDto()

    @Serializable
    @SerialName("command_result")
    data class CommandResult(val command: PrinterEventCommandDto) : PrinterEventDto()
}
