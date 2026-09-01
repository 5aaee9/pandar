// Generated from contracts/hub-client.openapi.json. Do not edit.
package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.Required
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class ArtifactFilamentDto(
    @SerialName("filament_id")
    @Required
    val filamentId: String?,
    @SerialName("tray_info_idx")
    val trayInfoIdx: String? = null,
    @SerialName("nozzle_id")
    val nozzleId: Int? = null,
    @SerialName("filament_type")
    @Required
    val filamentType: String?,
    @Required
    val color: String?,
    @SerialName("used_grams")
    @Required
    val usedGrams: Double?,
    @SerialName("used_meters")
    @Required
    val usedMeters: Double?
)

@Serializable
data class ArtifactPlateDto(
    @SerialName("plate_id")
    val plateId: Int,
    val name: String,
    @SerialName("estimated_time_seconds")
    @Required
    val estimatedTimeSeconds: Int?,
    @SerialName("filament_weight_grams")
    @Required
    val filamentWeightGrams: Double?,
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
    @Required
    val defaultPlateId: Int?,
    @SerialName("plate_count")
    val plateCount: Int,
    val plates: List<ArtifactPlateDto>,
    val warnings: List<String>
)

@Serializable
data class JobPrintDto(
    val status: PrintStatusDto,
    @SerialName("printer_state")
    @Required
    val printerState: String?,
    @SerialName("progress_percent")
    @Required
    val progressPercent: Int?,
    @SerialName("remaining_time_minutes")
    @Required
    val remainingTimeMinutes: Int?,
    @SerialName("current_layer")
    @Required
    val currentLayer: Int?,
    @SerialName("total_layers")
    @Required
    val totalLayers: Int?,
    @SerialName("active_file")
    @Required
    val activeFile: String?,
    @SerialName("last_progress_percent")
    @Required
    val lastProgressPercent: Int?,
    @SerialName("last_layer")
    @Required
    val lastLayer: Int?,
    @Required
    val error: String?,
    @SerialName("started_at")
    @Required
    val startedAt: String?,
    @SerialName("finished_at")
    @Required
    val finishedAt: String?,
    @SerialName("updated_at")
    @Required
    val updatedAt: String?
)

@Serializable
data class JobCommandDto(
    val id: String,
    val kind: String,
    val status: CommandStatusDto,
    @SerialName("uploaded_url")
    @Required
    val uploadedUrl: String?
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
    @Required
    val metadata: ArtifactMetadataDto?,
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
    @Required
    val nozzleId: Int?,
    @Required
    val sourceColor: String?
)

@Serializable
data class FilamentUsageDto(
    @SerialName("slot_index")
    val slotIndex: Int,
    val source: String,
    @SerialName("ams_id")
    @Required
    val amsId: String?,
    @SerialName("tray_id")
    @Required
    val trayId: String?,
    @SerialName("global_tray_id")
    @Required
    val globalTrayId: Int?,
    @SerialName("external_id")
    @Required
    val externalId: String?,
    @SerialName("filament_id")
    @Required
    val filamentId: String?,
    @SerialName("setting_id")
    @Required
    val settingId: String?,
    @SerialName("filament_type")
    @Required
    val filamentType: String?,
    @Required
    val color: String?,
    @SerialName("used_mm")
    @Required
    val usedMm: String?,
    @SerialName("used_grams")
    @Required
    val usedGrams: String?,
    val confidence: String
)

@Serializable
data class JobMaterialDto(
    @SerialName("ams_mapping")
    @Required
    val amsMapping: List<Int>?,
    @SerialName("ams_mapping2")
    @Required
    val amsMapping2: List<AmsMapping2Dto>?,
    @SerialName("ams_mapping_info")
    @Required
    val amsMappingInfo: List<AmsMappingInfoDto>?,
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
    @Required
    val error: String?,
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
