use super::*;
use crate::{
    grpc::AgentControlService,
    jobs::{DEFAULT_MAX_ARTIFACT_BYTES, JobStorageConfig},
    protocol::agent::v1::{
        AgentEvent, AgentHello, PrintJobReport, PrinterSnapshot,
        agent_control_client::AgentControlClient, agent_control_server::AgentControlServer,
        agent_event,
    },
    repositories::CreatePrintJob,
};
use pandar_core::AgentId;
use serde::{Deserialize, de::DeserializeOwned};
use tokio::net::TcpListener;
use tokio_stream::{
    StreamExt,
    wrappers::{ReceiverStream, TcpListenerStream},
};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use tonic::transport::Server;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrinterEventTicketResponse {
    ticket: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WebSocketPrinterEvent {
    #[serde(rename = "printer_snapshot")]
    PrinterSnapshot { printer: Box<EventPrinter> },
    #[serde(rename = "job_progress")]
    JobProgress { job: EventJob },
}

#[derive(Debug, Deserialize)]
struct EventPrinter {
    tenant_id: String,
    agent_id: String,
    serial_number: String,
    chamber_target_temperature_celsius: Option<String>,
    state_revision: u64,
    print: EventPrint,
}

#[derive(Debug, Deserialize)]
struct EventPrint {
    task_generation: u64,
    error_generation: u64,
    hms: Vec<crate::repositories::PrinterHms>,
    job_state: Option<u32>,
    gcode_state: Option<String>,
    task_id: Option<String>,
    subtask_id: Option<String>,
    progress_percent: Option<u8>,
    remaining_time_minutes: Option<u32>,
    current_layer: Option<u32>,
    total_layers: Option<u32>,
    gcode_file: Option<String>,
    subtask_name: Option<String>,
    print_error: Option<u32>,
    printer_job_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum OldShapePrinterEvent {
    #[serde(rename = "printer_snapshot")]
    PrinterSnapshot { printer: OldShapePrinter },
}

#[derive(Debug, Deserialize)]
struct OldShapePrinter {
    id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct EventJob {
    id: String,
    status: String,
    print: EventJobPrint,
}

#[derive(Debug, Deserialize)]
struct EventJobPrint {
    status: String,
    progress_percent: Option<u8>,
}

fn decode<T>(body: Value) -> T
where
    T: DeserializeOwned,
{
    decode_json(body)
}

fn decode_ws_message<T>(message: Message) -> T
where
    T: DeserializeOwned,
{
    match message {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        other => panic!("expected text websocket message, got {other:?}"),
    }
}

#[test]
fn printer_event_rolling_decode_accepts_legacy_and_old_shape_ignores_enrichment() {
    let legacy = serde_json::json!({
        "type": "printer_snapshot",
        "printer": {
            "id": "printer-1",
            "tenant_id": "00000000-0000-4000-8000-000000000001",
            "agent_id": "00000000-0000-4000-8000-000000000002",
            "serial_number": "SN-1",
            "name": "Printer",
            "model": null,
            "status": "IDLE",
            "last_seen_at": "2026-07-10T00:00:00Z",
            "created_at": "2026-07-10T00:00:00Z",
            "nozzle_temperatures": [],
            "active_nozzle": null,
            "bed_temperature_celsius": null,
            "bed_target_temperature_celsius": null,
            "chamber_temperature_celsius": null,
            "chamber_light_on": null,
            "materials": null
        }
    });
    let decoded: crate::printer_events::PrinterEvent =
        serde_json::from_value(legacy).expect("legacy snapshot should decode");
    let crate::printer_events::PrinterEvent::PrinterSnapshot { printer } = decoded else {
        panic!("expected legacy printer snapshot")
    };
    assert_eq!(printer.state_revision, None);
    assert_eq!(printer.print, None);

    let enriched = serde_json::json!({
        "type": "printer_snapshot",
        "printer": {
            "id": "printer-1",
            "status": "RUNNING",
            "state_revision": 9,
            "print": {
                "task_generation": 2,
                "error_generation": 3,
                "hms": [],
                "job_state": null,
                "gcode_state": "RUNNING",
                "task_id": null,
                "subtask_id": null,
                "progress_percent": null,
                "remaining_time_minutes": null,
                "current_layer": null,
                "total_layers": null,
                "gcode_file": null,
                "subtask_name": null,
                "print_error": null,
                "printer_job_id": null
            }
        }
    });
    let old: OldShapePrinterEvent =
        serde_json::from_value(enriched).expect("old shape should ignore additive fields");
    let OldShapePrinterEvent::PrinterSnapshot { printer } = old;
    assert_eq!(printer.id, "printer-1");
    assert_eq!(printer.status, "RUNNING");
}

#[tokio::test]
async fn printer_events_invalid_tenant_returns_bad_request_before_upgrade() {
    let (status, body) = request(
        app().await,
        Method::GET,
        "/api/v1/tenants/not-a-uuid/printer-events",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_tenant_id");
}

#[tokio::test]
async fn printer_events_missing_tenant_returns_not_found_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ws-viewer",
    )
    .await;
    let tenant_id = TenantId::new();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printer-events"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_forbidden");
}

#[tokio::test]
async fn printer_events_websocket_accepts_linked_viewer_jwt() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "linked-ws-viewer",
    )
    .await;
    let http_addr = serve_http(app).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());

    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    drop(ws);
}

#[tokio::test]
async fn printer_events_websocket_accepts_no_auth_without_ticket() {
    let state = state().await.with_no_auth_for_tests(true);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("no-auth-ws", "No Auth WS")
        .await
        .unwrap();
    let http_addr = serve_http(app).await;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    ))
    .await
    .unwrap();

    drop(ws);
}

#[tokio::test]
async fn printer_events_ticket_requires_linked_viewer() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let linked = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "ticket-viewer",
    )
    .await;
    let unlinked = jwt_for(
        "unlinked-ticket-viewer",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = request(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "missing_auth_token");

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &unlinked,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_forbidden");

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &linked,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<PrinterEventTicketResponse>(body);
    assert!(!body.ticket.is_empty());
    assert!(!body.expires_at.is_empty());
}

#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_once() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(app.clone()).await;
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);

    let err = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("401") || message.contains("Unauthorized"),
        "unexpected reused-ticket error: {message}"
    );
}

#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_from_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let app = router(sibling);
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sibling-ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(app.clone()).await;
    let (status, body) = request_as(
        router(state.clone()),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);
}

#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_from_separate_sqlite_connection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        temp_dir.path().join("pandar-ticket-test.db").display()
    );
    let issuer_storage = JobStorageConfig::new(
        temp_dir.path().join("issuer-spool"),
        DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let subscriber_storage = JobStorageConfig::new(
        temp_dir.path().join("subscriber-spool"),
        DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let issuer = AppState::connect_with_config_values(
        database_url.clone(),
        issuer_storage,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let subscriber = AppState::connect_with_config_values(
        database_url,
        subscriber_storage,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let tenant = issuer
        .tenants()
        .create("sqlite-file-acme", "SQLite File Acme")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &issuer,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sqlite-file-ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(router(subscriber)).await;
    let (status, body) = request_as(
        router(issuer),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);
}

#[tokio::test]
async fn printer_events_websocket_closes_immediately_when_process_epoch_changes() {
    let state = state().await;
    let tenant = state
        .tenants()
        .create("epoch-ws-acme", "Epoch WS Acme")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "epoch-ws-token",
    )
    .await;
    let http_addr = serve_http(router(state.clone())).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    state.printer_events().invalidate_epoch();

    assert_socket_closed_without_text(&mut ws, "epoch change").await;
}

#[tokio::test]
async fn epoch_change_after_serialization_closes_without_sending_stale_text() {
    let (state, tenant, token, http_addr) = epoch_window_fixture("after-serialization").await;
    let mut pause = crate::routes::printer_events::send_pause::install_after_serialization();
    let mut ws = connect_printer_events(http_addr, tenant.id, &token).await;
    state
        .printer_events()
        .publish_local(tenant.id, test_command_event("serialized"))
        .await;
    pause.wait_until_reached().await;

    state.printer_events().invalidate_epoch();
    pause.resume();

    assert_socket_closed_without_text(&mut ws, "epoch change after serialization").await;
}

#[tokio::test]
async fn epoch_change_cancels_a_blocked_websocket_flush() {
    let (state, tenant, token, http_addr) = epoch_window_fixture("blocked-flush").await;
    let mut pause = crate::routes::printer_events::send_pause::install_during_flush();
    let mut ws = connect_printer_events(http_addr, tenant.id, &token).await;
    state
        .printer_events()
        .publish_local(tenant.id, test_command_event("blocked"))
        .await;
    pause.wait_until_reached().await;

    state.printer_events().invalidate_epoch();

    assert_socket_closed_without_text(&mut ws, "epoch change during blocked flush").await;
    pause.resume();
}

#[tokio::test]
async fn printer_events_websocket_closes_immediately_when_tenant_receiver_lags() {
    let event_hub = crate::printer_events::PrinterEventHub::with_capacity_for_tests(1);
    let state = state()
        .await
        .with_printer_events_for_tests(event_hub.clone());
    let tenant = state
        .tenants()
        .create("lagged-ws-acme", "Lagged WS Acme")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "lagged-ws-token",
    )
    .await;
    let http_addr = serve_http(router(state)).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    event_hub
        .publish_local_burst_for_tests(
            tenant.id,
            vec![test_command_event("first"), test_command_event("second")],
        )
        .await;

    let next = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .expect("lagged receiver must close the websocket without waiting for another event");
    assert!(
        !matches!(next, Some(Ok(Message::Text(_)))),
        "lagged websocket must not resume from the newest buffered event"
    );
}

#[tokio::test]
async fn printer_events_cross_replica_ticket_safety_matrix() {
    let state = state().await;
    let subscriber = sibling_state(&state);
    let issuer = router(state.clone());
    let http_addr = serve_http(router(subscriber)).await;
    let tenant = state
        .tenants()
        .create("ticket-matrix-acme", "Ticket Matrix Acme")
        .await
        .unwrap();
    let other = state
        .tenants()
        .create("ticket-matrix-other", "Ticket Matrix Other")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-matrix-token",
    )
    .await;
    let other_token = auth_token_for_role(
        &state,
        &other.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-matrix-other-token",
    )
    .await;

    let ticket = issue_ticket(issuer.clone(), tenant.id, &token).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);

    assert_ws_ticket_rejected(http_addr, tenant.id, &ticket).await;

    let wrong_tenant_ticket = issue_ticket(issuer.clone(), other.id, &other_token).await;
    assert_ws_ticket_rejected(http_addr, tenant.id, &wrong_tenant_ticket).await;

    let expired_ticket = "pandar_ws_expired_matrix";
    seed_expired_ticket(state.database(), tenant.id, expired_ticket).await;
    assert_ws_ticket_rejected(http_addr, tenant.id, expired_ticket).await;
}

#[tokio::test]
async fn printer_events_websocket_rejects_invalid_ticket_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/printer-events?ticket=not-a-ticket",
            tenant.id
        ),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
}

#[tokio::test]
async fn printer_events_websocket_rejects_wrong_tenant_ticket_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant_a = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_b = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_a.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "tenant-a-ticket",
    )
    .await;
    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant_a.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (status, body) = request(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/printer-events?ticket={ticket}",
            tenant_b.id
        ),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
}

#[tokio::test]
async fn printer_events_unlinked_external_jwt_returns_forbidden_before_upgrade() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = jwt_for(
        "unlinked-ws-viewer",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/printer-events", tenant.id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_forbidden");
}

#[tokio::test]
async fn printer_events_websocket_receives_snapshot_from_grpc_stream() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    state
        .agents()
        .rotate_credential(
            tenant.id,
            agent.id,
            TEST_AGENT_CREDENTIAL,
            test_audit_actor(),
        )
        .await
        .unwrap();
    let http_addr = serve_http(app).await;
    let grpc_addr = serve_grpc(state).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    sender.send(hello_event(tenant.id, agent.id)).await.unwrap();
    let mut client = AgentControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let stream = client
        .reverse_connect(ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    sender
        .send(snapshot_event(tenant.id, agent.id))
        .await
        .unwrap();

    let message = ws.next().await.unwrap().unwrap();
    let body = decode_ws_message::<WebSocketPrinterEvent>(message);
    let WebSocketPrinterEvent::PrinterSnapshot { printer } = body else {
        panic!("expected printer snapshot websocket event");
    };
    assert_eq!(printer.tenant_id, tenant.id.to_string());
    assert_eq!(printer.agent_id, agent.id.to_string());
    assert_eq!(printer.serial_number, "SN-001");
    assert_eq!(
        printer.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );
    drop(stream);
}

#[tokio::test]
async fn printer_events_websocket_receives_job_progress_from_grpc_stream() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "job-progress-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    state
        .agents()
        .rotate_credential(
            tenant.id,
            agent.id,
            TEST_AGENT_CREDENTIAL,
            test_audit_actor(),
        )
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id: tenant.id,
            printer_id: printer_id.clone(),
            agent_id: agent.id,
            artifact_id: JOB_PROGRESS_ARTIFACT_ID.to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 3,
            artifact_storage_path: format!("{}/{JOB_PROGRESS_ARTIFACT_ID}/plate.3mf", tenant.id),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: true,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap();
    let http_addr = serve_http(app).await;
    let grpc_addr = serve_grpc(state).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (sender, receiver) = tokio::sync::mpsc::channel(8);
    sender.send(hello_event(tenant.id, agent.id)).await.unwrap();
    let mut client = AgentControlClient::connect(format!("http://{grpc_addr}"))
        .await
        .unwrap();
    let stream = client
        .reverse_connect(ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    sender
        .send(print_report_event(
            tenant.id,
            agent.id,
            format!("serial-{printer_id}"),
            created.job.id.to_string(),
            created.artifact.id,
        ))
        .await
        .unwrap();

    let message = ws.next().await.unwrap().unwrap();
    let body = decode_ws_message::<WebSocketPrinterEvent>(message);
    let WebSocketPrinterEvent::JobProgress { job } = body else {
        panic!("expected job progress websocket event");
    };
    assert_eq!(job.id, created.job.id.to_string());
    assert!(
        job.status == "queued" || job.status == "sent",
        "unexpected dispatch status: {}",
        job.status
    );
    assert_eq!(job.print.status, "running");
    assert_eq!(job.print.progress_percent, Some(66));
    drop(stream);
}

#[tokio::test]
async fn printer_events_websocket_receives_event_from_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(sibling);
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sibling-event-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .apply_print_report(crate::repositories::ApplyPrintReport {
            tenant_id: tenant.id,
            agent_id: agent.id,
            serial: format!("serial-{printer_id}"),
            task_id: Some("external-task".to_owned()),
            job_id: None,
            print_error: Some(83_918_929),
            printer_job_id: Some(String::new()),
            job_attr: Some(0x21),
            artifact_id: None,
            subtask_id: None,
            gcode_file: None,
            subtask_name: None,
            gcode_state: Some("RUNNING".to_owned()),
            percent: Some(66),
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            hms: Some(Vec::new()),
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-07-10T00:00:00Z".to_owned(),
        })
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let http_addr = serve_http(app).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    state
        .publish_printer_event(
            tenant.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    let message = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .expect("sibling websocket should receive event")
        .unwrap()
        .unwrap();
    let body = decode_ws_message::<WebSocketPrinterEvent>(message);
    let WebSocketPrinterEvent::PrinterSnapshot { printer } = body else {
        panic!("expected printer snapshot websocket event");
    };
    assert_eq!(printer.tenant_id, tenant.id.to_string());
    assert!(printer.state_revision > 1);
    assert_eq!(printer.print.task_generation, 1);
    assert_eq!(printer.print.error_generation, 1);
    assert_eq!(printer.print.job_state, Some(2));
    assert_eq!(printer.print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(printer.print.task_id.as_deref(), Some("external-task"));
    assert_eq!(printer.print.subtask_id, None);
    assert_eq!(printer.print.progress_percent, Some(66));
    assert_eq!(printer.print.remaining_time_minutes, None);
    assert_eq!(printer.print.current_layer, None);
    assert_eq!(printer.print.total_layers, None);
    assert_eq!(printer.print.gcode_file, None);
    assert_eq!(printer.print.subtask_name, None);
    assert_eq!(printer.print.print_error, Some(83_918_929));
    assert_eq!(printer.print.printer_job_id.as_deref(), Some(""));
    assert!(printer.print.hms.is_empty());
}

#[tokio::test]
async fn printer_events_websocket_receives_one_event_from_publishing_instance() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "single-event-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(tenant.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let http_addr = serve_http(app).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    state
        .publish_printer_event(
            tenant.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    let message = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .expect("websocket should receive event")
        .unwrap()
        .unwrap();
    assert!(matches!(message, Message::Text(_)));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), ws.next())
            .await
            .is_err(),
        "publishing instance should not deliver a duplicate event"
    );
}

#[tokio::test]
async fn printer_events_websocket_ignores_wrong_tenant_event_from_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(sibling);
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let other = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "wrong-tenant-event-ws-token",
    )
    .await;
    let agent = state
        .agents()
        .create(other.id, "other-agent")
        .await
        .unwrap();
    let printer_id = insert_printer_fixture(state.database(), other.id, agent.id)
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_with_live_status_for_tenant(other.id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let http_addr = serve_http(app).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    state
        .publish_printer_event(
            other.id,
            crate::printer_events::PrinterEvent::PrinterSnapshot {
                printer: Box::new(crate::printer_events::printer_event_printer(printer, None)),
            },
        )
        .await;

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), ws.next())
            .await
            .is_err(),
        "websocket should ignore wrong-tenant events"
    );
}

fn test_audit_actor() -> crate::repositories::AuditActor {
    crate::repositories::AuditActor::tenant_token(None, "test-setup-token", vec!["*"])
}

fn test_command_event(id: &str) -> crate::printer_events::PrinterEvent {
    crate::printer_events::PrinterEvent::CommandResult {
        command: Box::new(crate::printer_events::PrinterEventCommand {
            id: id.to_owned(),
            tenant_id: "tenant".to_owned(),
            agent_id: "agent".to_owned(),
            printer_id: None,
            kind: "test".to_owned(),
            status: "succeeded".to_owned(),
            payload_json: "{}".to_owned(),
            error: None,
            result_json: None,
            created_at: "2026-07-10T00:00:00Z".to_owned(),
            updated_at: "2026-07-10T00:00:00Z".to_owned(),
        }),
    }
}

async fn epoch_window_fixture(
    suffix: &str,
) -> (AppState, pandar_core::Tenant, String, std::net::SocketAddr) {
    let state = state().await;
    let tenant = state
        .tenants()
        .create(
            &format!("epoch-window-{suffix}"),
            &format!("Epoch Window {suffix}"),
        )
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        &format!("epoch-window-{suffix}-token"),
    )
    .await;
    let http_addr = serve_http(router(state.clone())).await;
    (state, tenant, token, http_addr)
}

async fn connect_printer_events(
    http_addr: std::net::SocketAddr,
    tenant_id: TenantId,
    token: &str,
) -> impl tokio_stream::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin
{
    let mut request = format!("ws://{http_addr}/api/v1/tenants/{tenant_id}/printer-events")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    tokio_tungstenite::connect_async(request).await.unwrap().0
}

async fn assert_socket_closed_without_text<S>(socket: &mut S, reason: &str)
where
    S: tokio_stream::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let next = tokio::time::timeout(std::time::Duration::from_secs(1), socket.next())
        .await
        .unwrap_or_else(|_| panic!("{reason} must close the websocket promptly"));
    match next {
        None | Some(Ok(Message::Close(_))) | Some(Err(_)) => {}
        Some(Ok(Message::Text(_))) => panic!("{reason} must not emit a stale text event"),
        Some(Ok(other)) => panic!("{reason} returned unexpected websocket frame {other:?}"),
    }
}

const JOB_PROGRESS_ARTIFACT_ID: &str = "22222222-2222-4222-8222-222222222222";

async fn issue_ticket(app: Router, tenant_id: TenantId, token: &str) -> String {
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printer-events/tickets"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    decode::<PrinterEventTicketResponse>(body).ticket
}

async fn assert_ws_ticket_rejected(
    http_addr: std::net::SocketAddr,
    tenant_id: TenantId,
    ticket: &str,
) {
    let err = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{tenant_id}/printer-events?ticket={ticket}",
    ))
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("401") || message.contains("Unauthorized"),
        "unexpected rejected-ticket error: {message}"
    );
}

async fn seed_expired_ticket(database: &crate::Database, tenant_id: TenantId, ticket: &str) {
    let now = time::OffsetDateTime::now_utc();
    let created_at = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let expires_at = (now - time::Duration::seconds(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let ticket_hash = crate::repositories::hash_secret(ticket);
    match database {
        crate::Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO printer_event_tickets (id, tenant_id, ticket_hash, created_at, expires_at, used_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(tenant_id.to_string())
            .bind(ticket_hash)
            .bind(created_at)
            .bind(expires_at)
            .execute(pool)
            .await
            .unwrap();
        }
        crate::Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO printer_event_tickets (id, tenant_id, ticket_hash, created_at, expires_at, used_at)
                 VALUES ($1, $2, $3, $4, $5, NULL)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(tenant_id.to_string())
            .bind(ticket_hash)
            .bind(created_at)
            .bind(expires_at)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn serve_http(app: Router) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

async fn serve_grpc(state: AppState) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        Server::builder()
            .add_service(AgentControlServer::new(AgentControlService::new(state)))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    addr
}

fn hello_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::Hello(AgentHello {
            name: "agent".to_string(),
            version: "0.1.0".to_string(),
            credential: TEST_AGENT_CREDENTIAL.to_string(),
            capabilities: Vec::new(),
        })),
    }
}

const TEST_AGENT_CREDENTIAL: &str = "pandar_ac_test";

fn snapshot_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: "SN-001".to_string(),
            host: "192.0.2.10".to_string(),
            access_code: "12345678".to_string(),
            name: "X1 Carbon".to_string(),
            model: "X1C".to_string(),
            state: "idle".to_string(),
            nozzle_temperatures: Vec::new(),
            active_nozzle: String::new(),
            bed_temperature_celsius: String::new(),
            bed_target_temperature_celsius: String::new(),
            chamber_temperature_celsius: String::new(),
            chamber_target_temperature_celsius: "45".to_owned(),
            chamber_light_on: None,
            device_features: None,
            connection_authoritative: false,
        })),
    }
}

fn print_report_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: String,
    job_id: String,
    artifact_id: String,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrintJobReport(PrintJobReport {
            serial,
            job_id,
            artifact_id,
            subtask_id: String::new(),
            gcode_file: "plate.3mf".to_string(),
            subtask_name: String::new(),
            gcode_state: "RUNNING".to_string(),
            percent: 66,
            has_percent: true,
            remaining_time_minutes: 12,
            has_remaining_time_minutes: true,
            current_layer: 2,
            has_current_layer: true,
            total_layers: 8,
            has_total_layers: true,
            hms: Vec::new(),
            has_hms: false,
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-06-22T10:00:00Z".to_string(),
            ..Default::default()
        })),
    }
}
