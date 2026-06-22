use super::*;
use crate::{
    grpc::AgentControlService,
    protocol::agent::v1::{
        AgentEvent, AgentHello, PrintJobReport, PrinterSnapshot,
        agent_control_client::AgentControlClient, agent_control_server::AgentControlServer,
        agent_event,
    },
    repositories::CreatePrintJob,
};
use pandar_core::AgentId;
use tokio::net::TcpListener;
use tokio_stream::{
    StreamExt,
    wrappers::{ReceiverStream, TcpListenerStream},
};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use tonic::transport::Server;

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
    assert_eq!(body, json!({ "error": "invalid_tenant_id" }));
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
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));
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
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));
}

#[tokio::test]
async fn printer_events_websocket_receives_snapshot_from_grpc_stream() {
    let state = state().await;
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
    let body: Value = match message {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        other => panic!("expected text websocket message, got {other:?}"),
    };

    assert_eq!(body["type"], "printer_snapshot");
    assert_eq!(body["printer"]["tenant_id"], tenant.id.to_string());
    assert_eq!(body["printer"]["agent_id"], agent.id.to_string());
    assert_eq!(body["printer"]["serial_number"], "SN-001");
    drop(stream);
}

#[tokio::test]
async fn printer_events_websocket_receives_job_progress_from_grpc_stream() {
    let state = state().await;
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
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: true,
            ams_mapping_json: None,
            ams_mapping2_json: None,
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
    let body: Value = match message {
        Message::Text(text) => serde_json::from_str(&text).unwrap(),
        other => panic!("expected text websocket message, got {other:?}"),
    };

    assert_eq!(body["type"], "job_progress");
    assert_eq!(body["job"]["id"], created.job.id.to_string());
    assert!(
        body["job"]["status"] == "queued" || body["job"]["status"] == "sent",
        "unexpected dispatch status: {}",
        body["job"]["status"]
    );
    assert_eq!(body["job"]["print"]["status"], "running");
    assert_eq!(body["job"]["print"]["progress_percent"], 66);
    drop(stream);
}

const JOB_PROGRESS_ARTIFACT_ID: &str = "22222222-2222-4222-8222-222222222222";

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
        })),
    }
}

fn snapshot_event(tenant_id: TenantId, agent_id: AgentId) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: "SN-001".to_string(),
            name: "X1 Carbon".to_string(),
            model: "X1C".to_string(),
            state: "idle".to_string(),
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
            diagnostics: Vec::new(),
            printer_materials_json: String::new(),
            observed_at: "2026-06-22T10:00:00Z".to_string(),
        })),
    }
}
