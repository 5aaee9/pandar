package zip.iptables.pandar.android.data.remote.dto

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import zip.iptables.pandar.android.data.remote.appJson

class PrinterListDtoTest {

    @Test fun rejects_printer_without_compatibility() {
        val json = """
        {"printers":[{
          "id":"p2","tenant_id":"t1","agent_id":"a1","serial_number":"SN002",
          "name":"Bare","model":null,"status":"offline",
          "last_seen_at":"2026-07-05T10:00:00Z","created_at":"2026-01-01T00:00:00Z",
          "materials":null
        }]}
        """.trimIndent()

        assertThrows(kotlinx.serialization.MissingFieldException::class.java) {
            appJson.decodeFromString<PrinterListDto>(json)
        }
    }

    @Test fun parses_full_printer_with_materials() {
        val json = """
        {"printers":[{
          "id":"p1","tenant_id":"t1","agent_id":"a1","serial_number":"SN001",
          "name":"Garage A2L","model":"A2L","compatibility":$UNKNOWN_PRINTER_COMPATIBILITY_JSON,"status":"running",
          "last_seen_at":"2026-07-05T10:00:00Z","created_at":"2026-01-01T00:00:00Z",
          "nozzle_temperatures":[
            {"label":"0","current_celsius":"220","target_celsius":"220"},
            {"label":"1","current_celsius":null,"target_celsius":null}
          ],
          "active_nozzle":"0",
          "bed_temperature_celsius":"60","bed_target_temperature_celsius":"60",
          "chamber_temperature_celsius":null,"chamber_light_on":true,
          "materials":{
            "ams_units":[{"unit_id":"0","unit_kind":"ams_lite_mixed","humidity":"2","trays":[
              {"tray_id":"0","type":"PLA","color":"#FFFFFF","name":"White","global_tray_id":24,"remaining_estimate":"100","exists":true}
            ]}],
            "external_spools":[{"external_id":"0","type":"PETG","color":"#000000","global_tray_id":4}],
            "active_tray":{"kind":"ams","global_tray_id":24},
            "observed_at":"2026-07-05T10:00:00Z"
          }
        }]}
        """.trimIndent()

        val dto = appJson.decodeFromString<PrinterListDto>(json)
        assertEquals(1, dto.printers.size)
        val p = dto.printers[0].toDomain()
        assertEquals("Garage A2L", p.name)
        assertEquals("A2L", p.model)
        assertEquals("running", p.status)
        assertEquals(true, p.chamberLightOn)
        assertEquals(2, p.nozzleTemperatures.size)
        assertEquals("220", p.nozzleTemperatures[0].currentCelsius)
        assertNull(p.nozzleTemperatures[1].currentCelsius)
        assertNotNull(p.materials)
        val materials = p.materials!!
        assertEquals(1, materials.amsUnits.size)
        assertEquals("0", materials.amsUnits[0].unitId)
        assertEquals("ams_lite_mixed", materials.amsUnits[0].unitKind)
        assertEquals(1, materials.amsUnits[0].trays.size)
        val tray = materials.amsUnits[0].trays[0]
        assertEquals("PLA", tray.type)
        assertEquals("#FFFFFF", tray.color)
        assertEquals(24, tray.globalTrayId)
        assertEquals(1, materials.externalSpools.size)
        assertEquals("PETG", materials.externalSpools[0].type)
        assertEquals(4, materials.externalSpools[0].globalTrayId)
        assertEquals("ams", materials.activeTray?.kind)
        assertEquals(24, materials.activeTray?.globalTrayId)
    }

    @Test fun parses_printer_with_null_materials_and_model() {
        val json = """
        {"printers":[{
          "id":"p2","tenant_id":"t1","agent_id":"a1","serial_number":"SN002",
          "name":"Bare","model":null,"compatibility":$UNKNOWN_PRINTER_COMPATIBILITY_JSON,"status":"offline",
          "last_seen_at":"2026-07-05T10:00:00Z","created_at":"2026-01-01T00:00:00Z",
          "materials":null
        }]}
        """.trimIndent()

        val dto = appJson.decodeFromString<PrinterListDto>(json)
        val p = dto.printers[0].toDomain()
        assertNull(p.model)
        assertNull(p.materials)
        assertEquals("offline", p.status)
        assertEquals(0, p.nozzleTemperatures.size)
    }
}
