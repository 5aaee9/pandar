package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class PauseRequest(
    @SerialName("action") @EncodeDefault val action: String = "pause",
)

@Serializable
data class ResumeRequest(
    @SerialName("action") @EncodeDefault val action: String = "resume",
)

@Serializable
data class StopRequest(
    @SerialName("action") @EncodeDefault val action: String = "stop",
)

@Serializable
data class ToggleLightRequest(
    @SerialName("action") @EncodeDefault val action: String = "toggle_light",
)

@Serializable
data class SetChamberLightRequest(
    @SerialName("action") @EncodeDefault val action: String = "set_chamber_light",
    @SerialName("light_on") val lightOn: Boolean,
)

@Serializable
data class SetHotendTemperatureRequest(
    @SerialName("action") @EncodeDefault val action: String = "set_hotend_temperature",
    @SerialName("temperature_celsius") val temperatureCelsius: Int,
    @SerialName("wait") val wait: Boolean,
    @SerialName("extruder_id") val extruderId: Int? = null,
)

@Serializable
data class SetBedTemperatureRequest(
    @SerialName("action") @EncodeDefault val action: String = "set_bed_temperature",
    @SerialName("temperature_celsius") val temperatureCelsius: Int,
    @SerialName("wait") val wait: Boolean,
)

@Serializable
data class SetChamberTemperatureRequest(
    @SerialName("action") @EncodeDefault val action: String = "set_chamber_temperature",
    @SerialName("temperature_celsius") val temperatureCelsius: Int,
    @SerialName("wait") val wait: Boolean,
)

@Serializable
data class AmsRereadRfidRequest(
    @SerialName("action") @EncodeDefault val action: String = "ams_reread_rfid",
    @SerialName("ams_id") val amsId: Int,
    @SerialName("slot_id") val slotId: Int,
)

@Serializable
data class AmsLoadFilamentRequest(
    @SerialName("action") @EncodeDefault val action: String = "ams_load_filament",
    @SerialName("ams_id") val amsId: Int,
    @SerialName("slot_id") val slotId: Int,
    @SerialName("global_tray_id") val globalTrayId: Int? = null,
    @SerialName("external_id") val externalId: String? = null,
    @SerialName("extruder_id") val extruderId: Int? = null,
)

@Serializable
data class AmsUnloadFilamentRequest(
    @SerialName("action") @EncodeDefault val action: String = "ams_unload_filament",
    @SerialName("ams_id") val amsId: Int,
    @SerialName("slot_id") val slotId: Int,
    @SerialName("global_tray_id") val globalTrayId: Int? = null,
    @SerialName("external_id") val externalId: String? = null,
    @SerialName("extruder_id") val extruderId: Int? = null,
)
