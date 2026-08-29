use std::{collections::HashSet, sync::Arc, time::Duration};

use pandar_core::{CommandId, CommandStatus};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use tokio::sync::{Mutex, mpsc};

use super::*;
use crate::{
    repositories::{PrintErrorAction, PrinterOperationKind, PrinterOperationPayload, UserRole},
    sessions::{AgentSession, SessionToken, empty_pending_live_commands},
};
use pandar_protocol::agent::v1::{
    AgentCapability, PrintErrorAction as ProtoPrintErrorAction, hub_command, printer_operation,
};

const ERROR_GENERATION: u64 = 9;
const BUILD_PLATE_MISMATCH: u32 = 83_918_929;
const BUILD_PLATE_MARKER_NOT_DETECTED: u32 = 83_918_946;
const BUILD_PLATE_OFFSET: u32 = 83_918_988;

mod authorization;
mod catalog;
mod concurrency;
mod single_flight;
mod state_validation;

struct RecoveryFixture {
    state: AppState,
    app: Router,
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: String,
    token: String,
    plugin_token: String,
    session_id: String,
    uri: String,
    plugin_uri: String,
    command_receiver: mpsc::Receiver<Result<pandar_protocol::agent::v1::HubCommand, tonic::Status>>,
    wake_receiver: mpsc::Receiver<()>,
}

impl RecoveryFixture {
    async fn new(
        slug: &str,
        serial_number: &str,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> Self {
        let state = state().await;
        Self::with_state(state, slug, serial_number, capabilities).await
    }

    async fn new_file(
        slug: &str,
        serial_number: &str,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> Self {
        let state = AppState::file_sqlite_for_tests().await.unwrap();
        Self::with_state(state, slug, serial_number, capabilities).await
    }

    async fn with_state(
        state: AppState,
        slug: &str,
        serial_number: &str,
        capabilities: impl IntoIterator<Item = AgentCapability>,
    ) -> Self {
        let app = router(state.clone());
        let tenant = state.tenants().create(slug, slug).await.unwrap();
        let agent = state.agents().create(tenant.id, "agent").await.unwrap();
        let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
            state.database(),
            tenant.id,
            agent.id,
            Some("A1"),
        )
        .await
        .unwrap();
        let session_token = SessionToken::new();
        let session_id = session_token.persisted_id();
        let now = pandar_core::created_at_now();
        state
            .agents()
            .claim_online_session(tenant.id, agent.id, &session_id, "test", &now)
            .await
            .unwrap();
        set_recovery_state(
            &state,
            &printer_id,
            serial_number,
            &session_id,
            Some(0x10),
            Some("job-7"),
        )
        .await;
        let (wake_sender, wake_receiver) = mpsc::channel(1);
        let (command_sender, command_receiver) = mpsc::channel(2);
        state
            .sessions()
            .register(AgentSession {
                token: session_token,
                tenant_id: tenant.id,
                agent_id: agent.id,
                name: "agent".to_owned(),
                version: "test".to_owned(),
                connected_at: now.clone(),
                last_heartbeat_at: now,
                wake_sender,
                close_sender: mpsc::channel(1).0,
                command_sender,
                capabilities: capabilities.into_iter().collect::<HashSet<_>>(),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(Mutex::new(())),
            })
            .await;
        let token = auth_token_for_role(
            &state,
            &tenant.id.to_string(),
            UserRole::Operator,
            &format!("{slug}-token"),
        )
        .await;
        let plugin_token =
            plugin_studio_tenant_token(&state, &tenant.id.to_string(), &format!("{slug}-plugin"))
                .await;
        let uri = format!(
            "/api/v1/tenants/{}/printers/{printer_id}/controls",
            tenant.id
        );
        let plugin_uri = format!("/api/v1/plugin/printers/{printer_id}/operations");
        Self {
            state,
            app,
            tenant_id: tenant.id,
            agent_id: agent.id,
            printer_id,
            token,
            plugin_token,
            session_id,
            uri,
            plugin_uri,
            command_receiver,
            wake_receiver,
        }
    }

    async fn request(&self, action: &str, generation: u64) -> (StatusCode, Value) {
        request_as(
            self.app.clone(),
            Method::POST,
            &self.uri,
            Some(recovery_body(action, generation)),
            &self.token,
        )
        .await
    }

    async fn plugin_request(&self, action: &str) -> (StatusCode, Value) {
        request_as(
            self.app.clone(),
            Method::POST,
            &self.plugin_uri,
            Some(plugin_recovery_body(action)),
            &self.plugin_token,
        )
        .await
    }
}

enum RecoveryMutation {
    PrintError(Option<i32>),
    ErrorGeneration(i64),
    ErrorTaskGeneration(Option<i64>),
    ErrorSession(Option<&'static str>),
    ErrorReceivedAt(Option<&'static str>),
    GcodeState(Option<&'static str>),
    CoarseState(&'static str),
    JobAttr(Option<i64>),
    PrinterJobId(Option<&'static str>),
}

async fn mutate_printer(fixture: &RecoveryFixture, mutation: RecoveryMutation) {
    let printer = crate::entities::printers::Entity::find_by_id(&fixture.printer_id)
        .one(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    match mutation {
        RecoveryMutation::PrintError(value) => active.print_error = Set(value),
        RecoveryMutation::ErrorGeneration(value) => active.print_error_generation = Set(value),
        RecoveryMutation::ErrorTaskGeneration(value) => {
            active.print_error_task_generation = Set(value)
        }
        RecoveryMutation::ErrorSession(value) => {
            active.print_error_session_id = Set(value.map(str::to_owned))
        }
        RecoveryMutation::ErrorReceivedAt(value) => {
            active.print_error_received_at = Set(value.map(str::to_owned))
        }
        RecoveryMutation::GcodeState(value) => {
            active.print_gcode_state = Set(value.map(str::to_owned))
        }
        RecoveryMutation::CoarseState(value) => active.status = Set(value.to_owned()),
        RecoveryMutation::JobAttr(value) => active.print_job_attr = Set(value),
        RecoveryMutation::PrinterJobId(value) => {
            active.print_job_id = Set(value.map(str::to_owned))
        }
    }
    active
        .update(&fixture.state.database().sea_orm_connection())
        .await
        .unwrap();
}

async fn set_recovery_state(
    state: &AppState,
    printer_id: &str,
    serial_number: &str,
    session_id: &str,
    job_attr: Option<i64>,
    printer_job_id: Option<&str>,
) {
    let printer = crate::entities::printers::Entity::find_by_id(printer_id)
        .one(&state.database().sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    let mut active = printer.into_active_model();
    active.serial_number = Set(serial_number.to_owned());
    active.status = Set("RUNNING".to_owned());
    active.print_task_generation = Set(ERROR_GENERATION as i64);
    active.print_error_generation = Set(ERROR_GENERATION as i64);
    active.print_job_attr = Set(job_attr);
    active.print_error_task_generation = Set(Some(ERROR_GENERATION as i64));
    active.print_error_session_id = Set(Some(session_id.to_owned()));
    active.print_error_received_at = Set(Some("2026-07-10T00:00:00Z".to_owned()));
    active.print_gcode_state = Set(Some("PAUSE".to_owned()));
    active.print_error = Set(Some(BUILD_PLATE_MISMATCH as i32));
    active.print_job_id = Set(printer_job_id.map(str::to_owned));
    active
        .update(&state.database().sea_orm_connection())
        .await
        .unwrap();
}

fn recovery_body(action: &str, generation: u64) -> Value {
    web_print_error_body(action, generation).unwrap()
}

fn plugin_recovery_body(action: &str) -> Value {
    serde_json::json!({
        "action": "handle_print_error",
        "error_action": action,
        "print_error": BUILD_PLATE_MISMATCH,
        "printer_job_id": "job-7",
        "sequence_id": 20_042
    })
}

fn assert_unavailable(status: StatusCode, body: Value) {
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "printer_operation_unavailable"
    );
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WebPrintErrorAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: String,
    error_action: PrintErrorAction,
    print_error: u32,
    printer_job_id: String,
    sequence_id: u64,
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}
