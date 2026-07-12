package zip.iptables.pandar.android.data.remote

import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import zip.iptables.pandar.android.data.remote.dto.AmsLoadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.AmsRereadRfidRequest
import zip.iptables.pandar.android.data.remote.dto.AmsUnloadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.HomeRequest
import zip.iptables.pandar.android.data.remote.dto.PauseRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis
import zip.iptables.pandar.android.data.remote.dto.ResumeRequest
import zip.iptables.pandar.android.data.remote.dto.SetBedTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberLightRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetHotendTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.StopRequest
import zip.iptables.pandar.android.data.remote.dto.ToggleLightRequest
import zip.iptables.pandar.android.data.remote.dto.moveAxisRequest

class ControlsBodyShapeTest {

    // Uses the SAME Json instance Retrofit's kotlinx-serialization converter uses.
    private val json = appJson

    @Test fun pause_is_minimal() =
        assertEquals("""{"action":"pause"}""", json.encodeToString(PauseRequest()))

    @Test fun no_polymorphic_discriminator_leaks() {
        val s = json.encodeToString(PauseRequest())
        assertFalse("type discriminator leaked", s.contains("\"type\""))
    }

    @Test fun resume_is_minimal() =
        assertEquals("""{"action":"resume"}""", json.encodeToString(ResumeRequest()))

    @Test fun stop_is_minimal() =
        assertEquals("""{"action":"stop"}""", json.encodeToString(StopRequest()))

    @Test fun home_all_axes_is_explicit() =
        assertEquals(
            """{"action":"home","axes":[]}""",
            json.encodeToString(HomeRequest()),
        )

    @Test fun every_axis_sign_and_step_maps_to_the_exact_request() {
        val cases = listOf(
            Triple(PrinterAxis.X, -10.0, 3000),
            Triple(PrinterAxis.X, -1.0, 3000),
            Triple(PrinterAxis.X, 1.0, 3000),
            Triple(PrinterAxis.X, 10.0, 3000),
            Triple(PrinterAxis.Y, -10.0, 3000),
            Triple(PrinterAxis.Y, -1.0, 3000),
            Triple(PrinterAxis.Y, 1.0, 3000),
            Triple(PrinterAxis.Y, 10.0, 3000),
            Triple(PrinterAxis.Z, -10.0, 900),
            Triple(PrinterAxis.Z, -1.0, 900),
            Triple(PrinterAxis.Z, 1.0, 900),
            Triple(PrinterAxis.Z, 10.0, 900),
        )

        cases.forEach { (axis, deltaMm, feedrate) ->
            val encoded = json.encodeToString(moveAxisRequest(axis, deltaMm))
            val expectedAxis = axis.name.lowercase()
            assertEquals(
                """{"action":"move_axes","movements":[{"axis":"$expectedAxis","delta_mm":$deltaMm}],"feedrate_mm_per_min":$feedrate}""",
                encoded,
            )
            assertFalse(encoded.contains("required_device_features"))
        }
    }

    @Test fun toggle_light_is_minimal() =
        assertEquals("""{"action":"toggle_light"}""", json.encodeToString(ToggleLightRequest()))

    @Test fun set_chamber_light() =
        assertEquals(
            """{"action":"set_chamber_light","light_on":true}""",
            json.encodeToString(SetChamberLightRequest(lightOn = true)),
        )

    @Test fun set_bed_temperature() =
        assertEquals(
            """{"action":"set_bed_temperature","temperature_celsius":60,"wait":false}""",
            json.encodeToString(SetBedTemperatureRequest(temperatureCelsius = 60, wait = false)),
        )

    @Test fun set_chamber_temperature() =
        assertEquals(
            """{"action":"set_chamber_temperature","temperature_celsius":40,"wait":true}""",
            json.encodeToString(SetChamberTemperatureRequest(temperatureCelsius = 40, wait = true)),
        )

    @Test fun set_hotend_temperature_omits_null_extruder() =
        assertEquals(
            """{"action":"set_hotend_temperature","temperature_celsius":220,"wait":true}""",
            json.encodeToString(SetHotendTemperatureRequest(temperatureCelsius = 220, wait = true)),
        )

    @Test fun set_hotend_temperature_with_extruder() =
        assertEquals(
            """{"action":"set_hotend_temperature","temperature_celsius":220,"wait":true,"extruder_id":0}""",
            json.encodeToString(SetHotendTemperatureRequest(temperatureCelsius = 220, wait = true, extruderId = 0)),
        )

    @Test fun ams_reread_rfid() =
        assertEquals(
            """{"action":"ams_reread_rfid","ams_id":1,"slot_id":2}""",
            json.encodeToString(AmsRereadRfidRequest(amsId = 1, slotId = 2)),
        )

    @Test fun ams_load_minimal() =
        assertEquals(
            """{"action":"ams_load_filament","ams_id":1,"slot_id":2}""",
            json.encodeToString(AmsLoadFilamentRequest(amsId = 1, slotId = 2)),
        )

    @Test fun ams_load_global_tray() =
        assertEquals(
            """{"action":"ams_load_filament","ams_id":1,"slot_id":2,"global_tray_id":5}""",
            json.encodeToString(AmsLoadFilamentRequest(amsId = 1, slotId = 2, globalTrayId = 5)),
        )

    @Test fun ams_load_external_id() =
        assertEquals(
            """{"action":"ams_load_filament","ams_id":1,"slot_id":2,"external_id":"ext1"}""",
            json.encodeToString(AmsLoadFilamentRequest(amsId = 1, slotId = 2, externalId = "ext1")),
        )

    @Test fun ams_unload_extruder() =
        assertEquals(
            """{"action":"ams_unload_filament","ams_id":1,"slot_id":2,"extruder_id":0}""",
            json.encodeToString(AmsUnloadFilamentRequest(amsId = 1, slotId = 2, extruderId = 0)),
        )

    @Test fun ams_unload_minimal() =
        assertEquals(
            """{"action":"ams_unload_filament","ams_id":1,"slot_id":2}""",
            json.encodeToString(AmsUnloadFilamentRequest(amsId = 1, slotId = 2)),
        )
}
