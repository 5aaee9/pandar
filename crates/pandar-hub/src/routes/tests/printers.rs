use super::*;
use pandar_core::AgentId;
use serde_json::json;

#[tokio::test]
async fn printer_list_returns_tenant_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["printers"][0]["id"], printer_id);
    assert_eq!(body["printers"][0]["tenant_id"], tenant_id.to_string());
    assert_eq!(body["printers"][0]["agent_id"], agent_id.to_string());
    assert_eq!(body["printers"][0]["materials"], serde_json::Value::Null);
}

#[tokio::test]
async fn printer_detail_returns_tenant_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], printer_id);
    assert_eq!(body["tenant_id"], tenant_id.to_string());
    assert_eq!(body["materials"], serde_json::Value::Null);
}

#[tokio::test]
async fn printer_routes_return_material_snapshots_without_credentials() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(crate::repositories::MaterialPatchInput {
            tenant_id,
            agent_id,
            printer_id: printer_id.clone(),
            serial_number: "serial".to_string(),
            printer_materials_json: json!({
                "type": "printer_material_patch",
                "observed_at": "2026-06-23T01:02:03Z",
                "ams_units": [{
                    "unit_id": "0",
                    "trays": [{
                        "tray_id": "0",
                        "filament_id": "GFL00",
                        "type": "PLA",
                        "color": "FF0000",
                        "access_token": "secret-token",
                        "auth": "secret-auth",
                        "passwd": "secret-passwd",
                        "access_code": "secret-access-code"
                    }]
                }],
                "external_spools": [{
                    "external_id": "254",
                    "tray_id": "0",
                    "type": "PETG"
                }],
                "active_tray": {
                    "kind": "ams",
                    "global_tray_id": 0,
                    "ams_id": "0",
                    "tray_id": "0"
                }
            })
            .to_string(),
        })
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["printers"][0]["materials"]["observed_at"],
        "2026-06-23T01:02:03Z"
    );
    assert_eq!(
        body["printers"][0]["materials"]["ams_units"][0]["unit_id"],
        "0"
    );
    assert_eq!(
        body["printers"][0]["materials"]["external_spools"][0]["external_id"],
        "254"
    );
    assert_eq!(
        body["printers"][0]["materials"]["active_tray"]["kind"],
        "ams"
    );
    assert!(!body.to_string().contains("secret-token"));
    assert!(!body.to_string().contains("secret-auth"));
    assert!(!body.to_string().contains("secret-passwd"));
    assert!(!body.to_string().contains("secret-access-code"));
    assert!(!body.to_string().contains("access_token"));
    assert!(!body.to_string().contains("auth"));
    assert!(!body.to_string().contains("passwd"));
    assert!(!body.to_string().contains("access_code"));

    let (status, detail) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["materials"], body["printers"][0]["materials"]);
}

#[tokio::test]
async fn missing_printer_detail_returns_not_found() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let printer_id = uuid::Uuid::new_v4();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "printer_not_found" }));
}

#[tokio::test]
async fn invalid_printer_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_printer_id" }));
}

#[tokio::test]
async fn refresh_printers_returns_command_record() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["tenant_id"], tenant_id);
    assert_eq!(body["agent_id"], agent_id);
    assert_eq!(body["kind"], "refresh_printers");
    assert_eq!(body["status"], "queued");
    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(tenant_id).unwrap())
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "agent.refresh_printers")
    );
}
