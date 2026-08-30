package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonClassDiscriminator

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
    data class CommandResult(val command: CommandResponseDto) : PrinterEventDto()
}
