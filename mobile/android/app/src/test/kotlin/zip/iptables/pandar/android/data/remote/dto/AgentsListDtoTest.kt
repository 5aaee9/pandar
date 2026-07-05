package zip.iptables.pandar.android.data.remote.dto

import org.junit.Assert.assertEquals
import org.junit.Test
import zip.iptables.pandar.android.data.remote.appJson

class AgentsListDtoTest {

    @Test fun parses_agents() {
        val json = """
        {"agents":[
          {"id":"a1","tenant_id":"t1","name":"local-agent","status":"online","created_at":"2026-01-01T00:00:00Z"},
          {"id":"a2","tenant_id":"t1","name":"remote-agent","status":"connecting","created_at":"2026-02-01T00:00:00Z"}
        ]}
        """.trimIndent()

        val dto = appJson.decodeFromString<AgentsListDto>(json)
        assertEquals(2, dto.agents.size)
        val agent = dto.agents[0].toDomain()
        assertEquals("a1", agent.id)
        assertEquals("local-agent", agent.name)
        assertEquals("online", agent.status)
        assertEquals("connecting", dto.agents[1].toDomain().status)
    }
}
