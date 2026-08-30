package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.MissingFieldException
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import zip.iptables.pandar.android.data.remote.appJson

class CommandResponseDtoTest {

    private val current = """
        {"id":"c1","tenant_id":"t1","agent_id":"a1","printer_id":null,
         "kind":"refresh_printers","status":"succeeded","payload_json":"{}",
         "error":null,"result_json":null,"created_at":"a","updated_at":"b"}
    """.trimIndent().replace("\n", "")

    @Test fun required_nullable_fields_accept_explicit_null() {
        val command = appJson.decodeFromString<CommandResponseDto>(current)

        assertNull(command.printerId)
        assertNull(command.error)
        assertNull(command.resultJson)
    }

    @Test fun required_nullable_fields_reject_omission() {
        for (field in listOf("printer_id", "error", "result_json")) {
            val missing = current.replace(Regex("\\\"$field\\\":null,?"), "")
            assertThrows("missing $field", MissingFieldException::class.java) {
                appJson.decodeFromString<CommandResponseDto>(missing)
            }
        }
    }
}
