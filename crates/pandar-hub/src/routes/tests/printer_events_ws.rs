use super::*;
use crate::{
    grpc::AgentControlService,
    jobs::{DEFAULT_MAX_ARTIFACT_BYTES, JobStorageConfig},
    repositories::CreatePrintJob,
};
use pandar_core::AgentId;
use pandar_protocol::agent::v1::{
    AgentEvent, AgentHello, PrintJobReport, PrinterSnapshot,
    agent_control_client::AgentControlClient, agent_control_server::AgentControlServer,
    agent_event,
};
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
    compatibility: pandar_core::DiagnosticCompatibility,
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

mod epoch;
mod grpc_events;
mod replication;
mod shape;
mod studio;
mod tickets;

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
            cooling_system: None,
            device_features: None,
            connection_authoritative: false,
            telemetry_authoritative: false,
            nozzle_system: None,
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
