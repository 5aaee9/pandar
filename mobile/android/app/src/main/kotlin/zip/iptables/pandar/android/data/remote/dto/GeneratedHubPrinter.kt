// Generated from contracts/hub-client.openapi.json. Do not edit.
package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.Required
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

@Serializable
enum class CapabilityDto(val wireValue: String) {
    @SerialName("supported")
    SUPPORTED("supported"),
    @SerialName("unsupported")
    UNSUPPORTED("unsupported"),
    @SerialName("unknown")
    UNKNOWN("unknown")
}

@Serializable
enum class NozzleLayoutDto(val wireValue: String) {
    @SerialName("single")
    SINGLE("single"),
    @SerialName("main_auxiliary")
    MAIN_AUXILIARY("main_auxiliary"),
    @SerialName("left_right")
    LEFT_RIGHT("left_right"),
    @SerialName("unknown")
    UNKNOWN("unknown")
}

@Serializable
enum class CoolingModeDto(val wireValue: String) {
    @SerialName("cooling")
    COOLING("cooling"),
    @SerialName("heating")
    HEATING("heating"),
    @SerialName("exhaust")
    EXHAUST("exhaust"),
    @SerialName("full_cooling")
    FULL_COOLING("full_cooling")
}

@Serializable
enum class CoolingFanKindDto(val wireValue: String) {
    @SerialName("hotend")
    HOTEND("hotend"),
    @SerialName("part_cooling")
    PART_COOLING("part_cooling"),
    @SerialName("auxiliary")
    AUXILIARY("auxiliary"),
    @SerialName("chamber")
    CHAMBER("chamber"),
    @SerialName("hotend_second")
    HOTEND_SECOND("hotend_second"),
    @SerialName("controller")
    CONTROLLER("controller"),
    @SerialName("inner_loop")
    INNER_LOOP("inner_loop"),
    @SerialName("auxiliary_second")
    AUXILIARY_SECOND("auxiliary_second")
}

@Serializable
data class PrinterNozzleTempDto(
    val label: String? = null,
    @SerialName("current_celsius")
    val currentCelsius: String? = null,
    @SerialName("target_celsius")
    val targetCelsius: String? = null,
    @SerialName("diameter_mm")
    val diameterMm: String? = null,
    @SerialName("nozzle_type")
    val nozzleType: String? = null
)

@Serializable
data class CalibrationOptionDto(
    val modes: List<Int>,
    @SerialName("default_mode")
    val defaultMode: Int
)

@Serializable
data class CompatibilityFeaturesDto(
    @SerialName("chamber_temperature")
    val chamberTemperature: CapabilityDto,
    val drying: CapabilityDto,
    @SerialName("dual_nozzle")
    val dualNozzle: CapabilityDto,
    @SerialName("flow_calibration")
    val flowCalibration: CapabilityDto,
    @SerialName("vibration_calibration")
    val vibrationCalibration: CapabilityDto,
    @SerialName("nozzle_offset_calibration")
    val nozzleOffsetCalibration: CapabilityDto,
    @SerialName("live_controls")
    val liveControls: CapabilityDto
)

@Serializable
data class PrintOptionCapabilitiesDto(
    val timelapse: Boolean,
    @SerialName("bed_leveling")
    @Required
    val bedLeveling: CalibrationOptionDto?,
    @SerialName("flow_calibration")
    @Required
    val flowCalibration: CalibrationOptionDto?,
    @SerialName("nozzle_offset_calibration")
    @Required
    val nozzleOffsetCalibration: CalibrationOptionDto?
)

@Serializable
data class PrinterCompatibilityDto(
    @SerialName("normalized_model")
    @Required
    val normalizedModel: String?,
    @SerialName("external_storage")
    val externalStorage: CapabilityDto,
    @SerialName("ftps_tls_1_2_cap")
    val ftpsTls_1_2Cap: Boolean,
    val features: CompatibilityFeaturesDto,
    @SerialName("print_options")
    val printOptions: PrintOptionCapabilitiesDto,
    @SerialName("chamber_fan")
    val chamberFan: CapabilityDto,
    @SerialName("nozzle_layout")
    val nozzleLayout: NozzleLayoutDto
)

@Serializable
data class CoolingFanDto(
    val kind: CoolingFanKindDto,
    @SerialName("speed_percent")
    val speedPercent: Int
)

@Serializable
data class CoolingSystemDto(
    val mode: CoolingModeDto? = null,
    val fans: List<CoolingFanDto>
)

@Serializable
data class AmsTrayDto(
    @SerialName("tray_id")
    val trayId: String? = null,
    val type: String? = null,
    val color: String? = null,
    @SerialName("multi_color")
    val multiColor: List<String>? = null,
    @SerialName("filament_id")
    val filamentId: String? = null,
    @SerialName("setting_id")
    val settingId: String? = null,
    val name: String? = null,
    @SerialName("remaining_estimate")
    val remainingEstimate: JsonElement? = null,
    @SerialName("k_value")
    val kValue: JsonElement? = null,
    val toolhead: String? = null,
    @SerialName("global_tray_id")
    val globalTrayId: Int? = null,
    val exists: Boolean? = null
)

@Serializable
data class AmsUnitDto(
    @SerialName("unit_id")
    val unitId: String? = null,
    @SerialName("unit_kind")
    val unitKind: String? = null,
    val humidity: JsonElement? = null,
    @SerialName("humidity_level")
    val humidityLevel: JsonElement? = null,
    @SerialName("temperature_celsius")
    val temperatureCelsius: JsonElement? = null,
    @SerialName("dry_status")
    val dryStatus: JsonElement? = null,
    @SerialName("dry_time_minutes")
    val dryTimeMinutes: JsonElement? = null,
    val toolhead: String? = null,
    val trays: List<AmsTrayDto>? = null
)

@Serializable
data class ExternalSpoolDto(
    @SerialName("external_id")
    val externalId: String? = null,
    @SerialName("tray_id")
    val trayId: String? = null,
    val type: String? = null,
    val color: String? = null,
    @SerialName("multi_color")
    val multiColor: List<String>? = null,
    @SerialName("filament_id")
    val filamentId: String? = null,
    @SerialName("setting_id")
    val settingId: String? = null,
    val name: String? = null,
    @SerialName("remaining_estimate")
    val remainingEstimate: JsonElement? = null,
    @SerialName("k_value")
    val kValue: JsonElement? = null,
    val toolhead: String? = null,
    @SerialName("global_tray_id")
    val globalTrayId: Int? = null,
    val exists: Boolean? = null
)

@Serializable
data class ActiveTrayDto(
    val kind: String? = null,
    @SerialName("ams_id")
    val amsId: String? = null,
    @SerialName("tray_id")
    val trayId: String? = null,
    @SerialName("global_tray_id")
    val globalTrayId: Int? = null,
    @SerialName("external_id")
    val externalId: String? = null
)

@Serializable
data class PrinterMaterialsDto(
    @SerialName("filament_switch_installed")
    val filamentSwitchInstalled: Boolean? = null,
    val cfg: String? = null,
    val aux: String? = null,
    val stat: String? = null,
    @SerialName("ams_units")
    val amsUnits: List<AmsUnitDto>,
    @SerialName("external_spools")
    val externalSpools: List<ExternalSpoolDto>,
    @SerialName("active_tray")
    @Required
    val activeTray: ActiveTrayDto?,
    @SerialName("observed_at")
    val observedAt: String
)

@Serializable
data class NozzleInfoDto(
    val id: Int,
    val diameter: Double,
    val type: String,
    val stat: Int? = null,
    @SerialName("fila_id")
    val filaId: String? = null,
    val wear: Int? = null,
    @SerialName("p_t")
    val pT: Int? = null,
    @SerialName("color_m")
    val colorM: String? = null
)

@Serializable
data class NozzleRackDto(
    val exist: Int? = null,
    val state: Int? = null,
    @SerialName("src_id")
    val srcId: Int? = null,
    @SerialName("tar_id")
    val tarId: Int? = null,
    val info: List<NozzleInfoDto>
)

@Serializable
data class NozzleHolderDto(
    val stat: Int? = null,
    val pos: Int? = null,
    val info: Int? = null
)

@Serializable
data class NozzleSystemDto(
    val nozzle: NozzleRackDto,
    val holder: NozzleHolderDto? = null
)

@Serializable
data class HmsDto(
    val attr: Int,
    val code: Int
)

@Serializable
data class PrinterPrintDto(
    @SerialName("task_generation")
    val taskGeneration: Long,
    @SerialName("error_generation")
    val errorGeneration: Long,
    val hms: List<HmsDto>,
    @SerialName("job_state")
    @Required
    val jobState: Int?,
    @SerialName("gcode_state")
    @Required
    val gcodeState: String?,
    @SerialName("task_id")
    @Required
    val taskId: String?,
    @SerialName("subtask_id")
    @Required
    val subtaskId: String?,
    @SerialName("subtask_name")
    @Required
    val subtaskName: String?,
    @SerialName("gcode_file")
    @Required
    val gcodeFile: String?,
    @SerialName("progress_percent")
    @Required
    val progressPercent: Int?,
    @SerialName("speed_level")
    @Required
    val speedLevel: Int?,
    @SerialName("remaining_time_minutes")
    @Required
    val remainingTimeMinutes: Int?,
    @SerialName("current_layer")
    @Required
    val currentLayer: Int?,
    @SerialName("total_layers")
    @Required
    val totalLayers: Int?,
    @SerialName("print_error")
    @Required
    val printError: Int?,
    @SerialName("printer_job_id")
    @Required
    val printerJobId: String?
)

@Serializable
data class PrinterDto(
    val id: String,
    @SerialName("tenant_id")
    val tenantId: String,
    @SerialName("agent_id")
    val agentId: String,
    @SerialName("serial_number")
    val serialNumber: String,
    val name: String,
    @Required
    val model: String?,
    val compatibility: PrinterCompatibilityDto,
    val status: String,
    @SerialName("last_seen_at")
    val lastSeenAt: String,
    @SerialName("created_at")
    val createdAt: String,
    @SerialName("nozzle_temperatures")
    val nozzleTemperatures: List<PrinterNozzleTempDto>? = null,
    @SerialName("active_nozzle")
    val activeNozzle: String? = null,
    @SerialName("bed_temperature_celsius")
    val bedTemperatureCelsius: String? = null,
    @SerialName("bed_target_temperature_celsius")
    val bedTargetTemperatureCelsius: String? = null,
    @SerialName("chamber_temperature_celsius")
    val chamberTemperatureCelsius: String? = null,
    @SerialName("chamber_target_temperature_celsius")
    val chamberTargetTemperatureCelsius: String? = null,
    @SerialName("chamber_light_on")
    val chamberLightOn: Boolean? = null,
    @SerialName("cooling_system")
    val coolingSystem: CoolingSystemDto? = null,
    @Required
    val materials: PrinterMaterialsDto?,
    @SerialName("nozzle_system")
    val nozzleSystem: NozzleSystemDto? = null,
    @SerialName("state_revision")
    val stateRevision: Long? = null,
    val print: PrinterPrintDto? = null
)

@Serializable
data class PrinterListDto(
    val printers: List<PrinterDto>
)
