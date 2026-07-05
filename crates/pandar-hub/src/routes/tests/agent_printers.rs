use super::*;

const AGENT_CREDENTIAL: &str = "pandar_ac_agent_printers";
const OTHER_AGENT_CREDENTIAL: &str = "pandar_ac_other_agent_printers";

#[tokio::test]
async fn agent_credential_lists_owned_printer_connections() {
    let state = state().await;
    let app = router(state.clone());
    let fixture = agent_printer_fixture(&state).await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/agents/{}/printers", fixture.agent_id),
        None,
        AGENT_CREDENTIAL,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["printers"].as_array().unwrap().len(), 1);
    assert_eq!(body["printers"][0]["serial"], fixture.serial);
    assert_eq!(body["printers"][0]["host"], "192.0.2.10");
    assert_eq!(body["printers"][0]["access_code"], "RESTORED-LINK-CODE");
    assert_eq!(body["printers"][0]["name"], "Fixture Printer");
    assert_eq!(body["printers"][0]["model"], serde_json::Value::Null);
}

#[tokio::test]
async fn agent_credential_cannot_list_another_agent_printers() {
    let state = state().await;
    let app = router(state.clone());
    let fixture = agent_printer_fixture(&state).await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/agents/{}/printers", fixture.agent_id),
        None,
        OTHER_AGENT_CREDENTIAL,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "forbidden" }));
}

struct AgentPrinterFixture {
    agent_id: pandar_core::AgentId,
    serial: String,
}

async fn agent_printer_fixture(state: &AppState) -> AgentPrinterFixture {
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let actor =
        crate::repositories::AuditActor::tenant_token(None, "agent-printer-route-test", vec!["*"]);
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    state
        .agents()
        .rotate_credential(tenant.id, agent.id, AGENT_CREDENTIAL, actor.clone())
        .await
        .unwrap();
    let other = state.agents().create(tenant.id, "other").await.unwrap();
    state
        .agents()
        .rotate_credential(tenant.id, other.id, OTHER_AGENT_CREDENTIAL, actor.clone())
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    state
        .printers()
        .update_details_with_audit(
            tenant.id,
            &printer_id,
            "Fixture Printer".to_owned(),
            "192.0.2.10".to_owned(),
            "RESTORED-LINK-CODE".to_owned(),
            actor,
        )
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_for_tenant(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();

    AgentPrinterFixture {
        agent_id: agent.id,
        serial: printer.serial_number,
    }
}
