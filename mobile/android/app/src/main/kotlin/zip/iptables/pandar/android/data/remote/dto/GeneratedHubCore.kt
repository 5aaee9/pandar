// Generated from contracts/hub-client.openapi.json. Do not edit.
package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
enum class CommandStatusDto(val wireValue: String) {
    @SerialName("queued")
    QUEUED("queued"),
    @SerialName("sent")
    SENT("sent"),
    @SerialName("acknowledged")
    ACKNOWLEDGED("acknowledged"),
    @SerialName("succeeded")
    SUCCEEDED("succeeded"),
    @SerialName("failed")
    FAILED("failed"),
    @SerialName("cancelled")
    CANCELLED("cancelled")
}

@Serializable
enum class JobStatusDto(val wireValue: String) {
    @SerialName("queued")
    QUEUED("queued"),
    @SerialName("sent")
    SENT("sent"),
    @SerialName("acknowledged")
    ACKNOWLEDGED("acknowledged"),
    @SerialName("succeeded")
    SUCCEEDED("succeeded"),
    @SerialName("failed")
    FAILED("failed"),
    @SerialName("cancelled")
    CANCELLED("cancelled")
}

@Serializable
enum class PrintStatusDto(val wireValue: String) {
    @SerialName("pending")
    PENDING("pending"),
    @SerialName("stalled")
    STALLED("stalled"),
    @SerialName("running")
    RUNNING("running"),
    @SerialName("completed")
    COMPLETED("completed"),
    @SerialName("failed")
    FAILED("failed"),
    @SerialName("cancelled")
    CANCELLED("cancelled")
}

@Serializable
data class AgentDto(
    val id: String,
    @SerialName("tenant_id")
    val tenantId: String,
    val name: String,
    val status: String,
    @SerialName("created_at")
    val createdAt: String
)

@Serializable
data class AgentsListDto(
    val agents: List<AgentDto>
)

@Serializable
data class CommandResponseDto(
    val id: String,
    @SerialName("tenant_id")
    val tenantId: String,
    @SerialName("agent_id")
    val agentId: String,
    @SerialName("printer_id")
    val printerId: String? = null,
    val kind: String,
    val status: CommandStatusDto,
    @SerialName("payload_json")
    val payloadJson: String,
    val error: String? = null,
    @SerialName("result_json")
    val resultJson: String? = null,
    @SerialName("created_at")
    val createdAt: String,
    @SerialName("updated_at")
    val updatedAt: String
)

@Serializable
data class MobileTicketExchangeRequest(
    val ticket: String,
    @SerialName("code_verifier")
    val codeVerifier: String
)

@Serializable
data class MobileAuthProfileDto(
    @SerialName("user_id")
    val userId: String,
    @SerialName("user_name")
    val userName: String,
    @SerialName("tenant_id")
    val tenantId: String,
    @SerialName("tenant_name")
    val tenantName: String
)

@Serializable
data class MobileTicketExchangeResponse(
    val token: String,
    @SerialName("expires_at")
    val expiresAt: String,
    val profile: MobileAuthProfileDto
)

@Serializable
enum class PrinterAxisRequest(val wireValue: String) {
    @SerialName("x")
    X("x"),
    @SerialName("y")
    Y("y"),
    @SerialName("z")
    Z("z")
}

@Serializable
data class PrinterAxisMovementRequest(
    val axis: PrinterAxisRequest,
    @SerialName("delta_mm")
    val deltaMm: Double
)

@Serializable
data class PrinterControlRequest(
    val action: String,
    @SerialName("light_on")
    val lightOn: Boolean? = null,
    val axes: List<PrinterAxisRequest>? = null,
    val movements: List<PrinterAxisMovementRequest>? = null,
    @SerialName("feedrate_mm_per_min")
    val feedrateMmPerMin: Int? = null,
    @SerialName("speed_mode")
    val speedMode: Int? = null,
    @SerialName("fan_index")
    val fanIndex: Int? = null,
    @SerialName("speed_percent")
    val speedPercent: Int? = null,
    val airduct: Boolean? = null,
    @SerialName("temperature_celsius")
    val temperatureCelsius: Int? = null,
    val wait: Boolean? = null,
    @SerialName("ams_id")
    val amsId: Int? = null,
    @SerialName("slot_id")
    val slotId: Int? = null,
    @SerialName("global_tray_id")
    val globalTrayId: Int? = null,
    @SerialName("external_id")
    val externalId: String? = null,
    @SerialName("duration_hours")
    val durationHours: Int? = null,
    val filament: String? = null,
    @SerialName("rotate_tray")
    val rotateTray: Boolean? = null,
    @SerialName("holder_action")
    val holderAction: Int? = null,
    @SerialName("nozzle_id")
    val nozzleId: Int? = null,
    @SerialName("extruder_id")
    val extruderId: Int? = null,
    @SerialName("error_action")
    val errorAction: PrintErrorActionDto? = null,
    @SerialName("error_generation")
    val errorGeneration: Long? = null,
    @SerialName("required_device_features")
    val requiredDeviceFeatures: List<RequiredDeviceFeatureDto>? = null
)

@Serializable
enum class PrintErrorActionDto(val wireValue: String) {
    @SerialName("resume")
    RESUME("resume"),
    @SerialName("ignore")
    IGNORE("ignore"),
    @SerialName("stop")
    STOP("stop")
}

@Serializable
enum class RequiredDeviceFeatureDto(val wireValue: String) {
    @SerialName("bambu_mqtt_homing")
    BAMBU_MQTT_HOMING("bambu_mqtt_homing"),
    @SerialName("bambu_mqtt_axis_control")
    BAMBU_MQTT_AXIS_CONTROL("bambu_mqtt_axis_control")
}
