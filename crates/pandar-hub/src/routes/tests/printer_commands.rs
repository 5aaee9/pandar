use super::*;
use pandar_core::AgentId;
use requests::{
    PrinterControlRequest, diagnose_printer_body, diagnose_printer_with_access_code_body,
    discover_printers_body, discover_printers_timeout_string_body, empty_body, move_axis,
    printer_control_body, printer_control_value, printer_discovery_result_json,
};
use serde::Deserialize;
use tokio::sync::mpsc;

mod requests;

#[derive(Debug, Deserialize)]
struct TenantResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AgentResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CommandResponse {
    id: String,
    agent_id: String,
    printer_id: Option<String>,
    kind: String,
    payload_json: String,
    result_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscoverPrintersPayload {
    timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct DiagnosePrinterPayload {
    serial_number: String,
}

#[derive(Debug, Deserialize)]
struct PrinterOperationPayload {
    printer_id: String,
    serial_number: String,
    operation: PrinterOperationPayloadDetails,
}

#[derive(Debug, Deserialize)]
struct PrinterOperationPayloadDetails {
    #[serde(rename = "type")]
    kind: String,
    speed_mode: Option<u8>,
    extruder_id: Option<u32>,
    on: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PrinterControlAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    speed_mode: u8,
}

#[derive(Debug, Deserialize)]
struct TenantTokenAuditMetadata {
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn discover_printers_requires_operator_role() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-discover-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/agents/{}/discover-printers",
            tenant.id, agent.id
        ),
        discover_printers_body(5),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn discover_printers_rejects_invalid_timeout_payloads() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    for payload in [
        discover_printers_body(0).unwrap(),
        discover_printers_body(16).unwrap(),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_discovery_timeout"
        );
    }

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        discover_printers_timeout_string_body("bad"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"
                ))
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: ErrorResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(body.error, "bad_request");
}

#[tokio::test]
async fn discover_printers_defaults_timeout_audits_and_wakes_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        empty_body(),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "discover_printers");
    assert_eq!(body.result_json, None);
    let payload: DiscoverPrintersPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.timeout_seconds, 5);
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "agent.discover_printers")
    );
}

#[tokio::test]
async fn discover_printers_defaults_empty_json_body() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"
                ))
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: CommandResponse = serde_json::from_slice(&body).unwrap();
    let payload: DiscoverPrintersPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.timeout_seconds, 5);
}

#[tokio::test]
async fn diagnose_printer_rejects_access_code_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let access_code = "ACCESS-CODE-SHOULD-NOT-LEAK";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        diagnose_printer_with_access_code_body("BAMBU123", access_code),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains(access_code));
}

#[tokio::test]
async fn diagnose_printer_enqueues_redacted_payload_audits_and_wakes_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        diagnose_printer_body("BAMBU123"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "diagnose_printer");
    let payload: DiagnosePrinterPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.serial_number, "BAMBU123");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.diagnose_printer")
        .expect("diagnostic audit event");
    let metadata: TenantTokenAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert!(!metadata.tenant_token_id.is_empty());
    assert_eq!(metadata.tenant_token_scopes, vec!["*".to_string()]);
}

#[tokio::test]
async fn refresh_printers_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<CommandResponse>(body).kind, "refresh_printers");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
    let command = state
        .commands()
        .next_queued_for_agent(tenant_id, agent_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(command.kind, "refresh_printers");
}

#[tokio::test]
async fn discover_printers_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        discover_printers_body(5),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<CommandResponse>(body).kind, "discover_printers");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn diagnose_printer_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        diagnose_printer_body("BAMBU123"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<CommandResponse>(body).kind, "diagnose_printer");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn printer_control_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-control-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/printers/{printer_id}/controls",
            tenant.id
        ),
        printer_control_body(PrinterControlRequest::action("pause")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn printer_control_enqueues_audits_and_wakes_owning_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_control_body(PrinterControlRequest::set_print_speed(4)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    assert_eq!(body.agent_id, agent_id.to_string());
    assert_eq!(body.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(payload.serial_number, format!("serial-{printer_id}"));
    assert_eq!(payload.operation.kind, "set_print_speed");
    assert_eq!(payload.operation.speed_mode, Some(4));
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("owning agent should be woken")
        .expect("wake channel should stay open");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.dispatch_control")
        .expect("printer control audit event");
    assert_eq!(event.target_type, "printer");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
    let metadata: PrinterControlAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(metadata.agent_id, agent_id.to_string());
    assert_eq!(metadata.serial_number, format!("serial-{printer_id}"));
    assert_eq!(metadata.action, "set_print_speed");
    assert_eq!(metadata.speed_mode, 4);
}

#[tokio::test]
async fn printer_control_rejects_unknown_model_before_command_or_audit_insert() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Mystery Model"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_control_body(PrinterControlRequest::action("pause")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "printer_control_unavailable"
    );
    assert_eq!(state.commands().count().await.unwrap(), 0);
    assert_no_printer_control_audit(&state, tenant_id).await;
}

#[tokio::test]
async fn printer_control_wakes_owning_agent_not_sibling() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let owner_agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let other_agent = state
        .agents()
        .create(tenant_id, "other-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        owner_agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    let (owner_wake_sender, mut owner_wake_receiver) = mpsc::channel(1);
    let (other_wake_sender, mut other_wake_receiver) = mpsc::channel(1);
    let (owner_close_sender, _) = mpsc::channel(1);
    let (other_close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: owner_agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: owner_wake_sender,
            close_sender: owner_close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: other_agent.id,
            name: "other-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: other_wake_sender,
            close_sender: other_close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_control_body(PrinterControlRequest::action("resume")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode::<CommandResponse>(body).agent_id,
        owner_agent_id.to_string()
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        owner_wake_receiver.recv(),
    )
    .await
    .expect("owning agent should be woken")
    .expect("wake channel should stay open");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            other_wake_receiver.recv()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn printer_control_rejects_invalid_action_and_speed_payloads() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();

    for payload in [
        printer_control_value(PrinterControlRequest::action("dance")),
        printer_control_value(PrinterControlRequest::action("set_print_speed")),
        printer_control_value(PrinterControlRequest::set_print_speed(0)),
        printer_control_value(PrinterControlRequest::set_print_speed(5)),
        printer_control_value(PrinterControlRequest::action("select_extruder")),
        printer_control_value(PrinterControlRequest::select_extruder(2)),
        printer_control_value(PrinterControlRequest::action("pause").with_speed_mode(2)),
        printer_control_value(PrinterControlRequest::action("pause").with_raw_command("M400")),
        printer_control_value(PrinterControlRequest::move_axes(Vec::new(), None)),
        printer_control_value(PrinterControlRequest::move_axes(
            vec![move_axis("x", 0.0)],
            None,
        )),
        printer_control_value(PrinterControlRequest::move_axes(
            vec![move_axis("a", 5.0)],
            None,
        )),
        printer_control_value(PrinterControlRequest::move_axes(
            vec![move_axis("x", 5.0), move_axis("x", 6.0)],
            None,
        )),
        printer_control_value(PrinterControlRequest::set_hotend_temperature(
            301, None, None,
        )),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_printer_control"
        );
        assert_eq!(state.commands().count().await.unwrap(), 0);
        assert_no_printer_control_audit(&state, tenant_id).await;
    }
}

#[tokio::test]
async fn printer_control_accepts_semantic_home_move_and_hotend_operations() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();

    for (payload, expected_type) in [
        (
            printer_control_value(PrinterControlRequest::home(vec!["x", "z"])),
            "home",
        ),
        (
            printer_control_value(PrinterControlRequest::move_axes(
                vec![move_axis("x", 10.0), move_axis("z", -1.0)],
                Some(1200),
            )),
            "move_axes",
        ),
        (
            printer_control_value(PrinterControlRequest::set_hotend_temperature(
                215,
                Some(true),
                Some(1),
            )),
            "set_hotend_temperature",
        ),
        (
            printer_control_value(PrinterControlRequest::set_temperature(
                "set_bed_temperature",
                75,
            )),
            "set_bed_temperature",
        ),
        (
            printer_control_value(PrinterControlRequest::set_temperature(
                "set_chamber_temperature",
                45,
            )),
            "set_chamber_temperature",
        ),
        (
            printer_control_value(PrinterControlRequest::set_chamber_light(true)),
            "set_chamber_light",
        ),
        (
            printer_control_value(PrinterControlRequest::action("toggle_light")),
            "toggle_light",
        ),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let body = decode::<CommandResponse>(body);
        assert_eq!(body.kind, "printer_operation");
        let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
        assert_eq!(payload.operation.kind, expected_type);
        if expected_type == "set_hotend_temperature" {
            assert_eq!(payload.operation.extruder_id, Some(1));
        }
        if expected_type == "set_chamber_light" {
            assert_eq!(payload.operation.on, Some(true));
        }
    }
}

async fn assert_no_printer_control_audit(state: &AppState, tenant_id: TenantId) {
    assert!(
        state
            .audit_events()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .iter()
            .all(|event| event.action != "printer.dispatch_control")
    );
}

#[tokio::test]
async fn command_detail_requires_viewer_and_returns_result_json() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let viewer_token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-command-detail",
    )
    .await;
    let command = state
        .commands()
        .enqueue_discover_printers(
            tenant.id,
            agent.id,
            crate::repositories::DiscoverPrintersPayload { timeout_seconds: 5 },
        )
        .await
        .unwrap();
    let result_json = printer_discovery_result_json();
    state
        .commands()
        .mark_sent(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .commands()
        .mark_succeeded_with_result(command.id, tenant.id, agent.id, Some(result_json.clone()))
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/commands/{}", tenant.id, command.id),
        None,
        &viewer_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.id, command.id.to_string());
    assert_eq!(body.result_json.as_deref(), Some(result_json.as_str()));

    let other_tenant = state.tenants().create("other", "Other Labs").await.unwrap();
    let other_token = auth_token_for_role(
        &state,
        &other_tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "other-command-detail",
    )
    .await;
    let (status, body) = request_as(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/commands/{}",
            other_tenant.id, command.id
        ),
        None,
        &other_token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "command_not_found");
}

#[tokio::test]
async fn invalid_command_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/commands/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_command_id");
}

#[tokio::test]
async fn invalid_agent_id_on_refresh_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/not-a-uuid/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_agent_id");
}
