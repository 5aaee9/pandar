// Generated from contracts/hub-client.openapi.json. Do not edit.
package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class ArtifactFilamentDto(
    @SerialName("filament_id")
    val filamentId: String? = null,
    @SerialName("tray_info_idx")
    val trayInfoIdx: String? = null,
    @SerialName("nozzle_id")
    val nozzleId: Int? = null,
    @SerialName("filament_type")
    val filamentType: String? = null,
    val color: String? = null,
    @SerialName("used_grams")
    val usedGrams: Double? = null,
    @SerialName("used_meters")
    val usedMeters: Double? = null
)

@Serializable
data class ArtifactPlateDto(
    @SerialName("plate_id")
    val plateId: Int,
    val name: String,
    @SerialName("estimated_time_seconds")
    val estimatedTimeSeconds: Int? = null,
    @SerialName("filament_weight_grams")
    val filamentWeightGrams: Double? = null,
    @SerialName("object_count")
    val objectCount: Int,
    val objects: List<String>,
    val filaments: List<ArtifactFilamentDto>,
    @SerialName("has_thumbnail")
    val hasThumbnail: Boolean
)

@Serializable
data class ArtifactMetadataDto(
    val source: String,
    @SerialName("display_name")
    val displayName: String,
    @SerialName("default_plate_id")
    val defaultPlateId: Int? = null,
    @SerialName("plate_count")
    val plateCount: Int,
    val plates: List<ArtifactPlateDto>,
    val warnings: List<String>
)

@Serializable
data class JobPrintDto(
    val status: PrintStatusDto,
    @SerialName("printer_state")
    val printerState: String? = null,
    @SerialName("progress_percent")
    val progressPercent: Int? = null,
    @SerialName("remaining_time_minutes")
    val remainingTimeMinutes: Int? = null,
    @SerialName("current_layer")
    val currentLayer: Int? = null,
    @SerialName("total_layers")
    val totalLayers: Int? = null,
    @SerialName("active_file")
    val activeFile: String? = null,
    @SerialName("last_progress_percent")
    val lastProgressPercent: Int? = null,
    @SerialName("last_layer")
    val lastLayer: Int? = null,
    val error: String? = null,
    @SerialName("started_at")
    val startedAt: String? = null,
    @SerialName("finished_at")
    val finishedAt: String? = null,
    @SerialName("updated_at")
    val updatedAt: String? = null
)

@Serializable
data class JobCommandDto(
    val id: String,
    val kind: String,
    val status: CommandStatusDto
)

@Serializable
data class JobArtifactDto(
    val id: String,
    @SerialName("tenant_id")
    val tenantId: String,
    val filename: String,
    @SerialName("content_type")
    val contentType: String,
    @SerialName("size_bytes")
    val sizeBytes: Long,
    val metadata: ArtifactMetadataDto? = null,
    @SerialName("created_at")
    val createdAt: String
)

@Serializable
data class AmsMapping2Dto(
    @SerialName("ams_id")
    val amsId: Int,
    @SerialName("slot_id")
    val slotId: Int
)

@Serializable
data class AmsMappingInfoDto(
    val ams: Int,
    val targetColor: String,
    val filamentId: String,
    val filamentType: String,
    val nozzleId: Int? = null,
    val sourceColor: String? = null
)

@Serializable
data class FilamentUsageDto(
    @SerialName("slot_index")
    val slotIndex: Int,
    val source: String,
    @SerialName("ams_id")
    val amsId: String? = null,
    @SerialName("tray_id")
    val trayId: String? = null,
    @SerialName("global_tray_id")
    val globalTrayId: Int? = null,
    @SerialName("external_id")
    val externalId: String? = null,
    @SerialName("filament_id")
    val filamentId: String? = null,
    @SerialName("setting_id")
    val settingId: String? = null,
    @SerialName("filament_type")
    val filamentType: String? = null,
    val color: String? = null,
    @SerialName("used_mm")
    val usedMm: String? = null,
    @SerialName("used_grams")
    val usedGrams: String? = null,
    val confidence: String
)

@Serializable
data class JobMaterialDto(
    @SerialName("ams_mapping")
    val amsMapping: List<Int>? = null,
    @SerialName("ams_mapping2")
    val amsMapping2: List<AmsMapping2Dto>? = null,
    @SerialName("ams_mapping_info")
    val amsMappingInfo: List<AmsMappingInfoDto>? = null,
    @SerialName("filament_usage")
    val filamentUsage: List<FilamentUsageDto>
)

@Serializable
data class JobDto(
    val id: String,
    @SerialName("tenant_id")
    val tenantId: String,
    @SerialName("printer_id")
    val printerId: String,
    @SerialName("agent_id")
    val agentId: String,
    @SerialName("artifact_id")
    val artifactId: String,
    @SerialName("command_id")
    val commandId: String,
    val status: JobStatusDto,
    val error: String? = null,
    @SerialName("created_at")
    val createdAt: String,
    @SerialName("updated_at")
    val updatedAt: String,
    val print: JobPrintDto,
    val command: JobCommandDto,
    val artifact: JobArtifactDto,
    val material: JobMaterialDto
)

@Serializable
data class JobListDto(
    val jobs: List<JobDto>
)

@Serializable
data class RecoveryReasonRequestDto(
    val reason: String? = null
)

@Serializable
data class ReprintJobRequestDto(
    val reason: String? = null,
    @SerialName("printer_id")
    val printerId: String? = null,
    @SerialName("plate_id")
    val plateId: Int? = null,
    @SerialName("use_ams")
    val useAms: Boolean? = null,
    @SerialName("bed_leveling")
    val bedLeveling: Boolean? = null,
    @SerialName("auto_bed_leveling")
    val autoBedLeveling: Int? = null,
    @SerialName("flow_cali")
    val flowCali: Boolean? = null,
    @SerialName("auto_flow_cali")
    val autoFlowCali: Int? = null,
    @SerialName("auto_offset_cali")
    val autoOffsetCali: Int? = null,
    val timelapse: Boolean? = null,
    @SerialName("ams_mapping")
    val amsMapping: List<Int>? = null,
    @SerialName("ams_mapping2")
    val amsMapping2: List<AmsMapping2Dto>? = null,
    @SerialName("ams_mapping_info")
    val amsMappingInfo: List<AmsMappingInfoDto>? = null
)
