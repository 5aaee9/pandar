use std::{collections::HashSet, sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header::AUTHORIZATION},
};
use pandar_core::{AgentId, CommandId, PrinterFirmwareModule, TenantId};
use serde_json::Value;
use tokio::sync::mpsc;
use tonic::Status;
use tower::ServiceExt;

use super::super::*;
use super::event_helpers::{control_result, control_result_with_status};
use crate::sessions::{AgentSession, SessionToken, empty_pending_live_commands};
use pandar_protocol::agent::v1::{
    AgentCapability, AgentEvent, CommandResult, FirmwareAcknowledgement, FirmwarePrepared,
    FirmwarePublished, FirmwareRefreshedModules, HubCommand, PrinterFirmwareInvalidated,
    PrinterFirmwareModule as ProtoPrinterFirmwareModule,
    PrinterFirmwareStatus as ProtoPrinterFirmwareStatus, agent_event, firmware_command_result,
};

pub(super) const GENERATION: u64 = 7;
pub(super) const URL_SENTINEL: &str =
    "https://user:secret@firmware.invalid/main.bin?signature=TASK6-URL-SENTINEL";

pub(super) struct FirmwareRouteFixture {
    pub state: AppState,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: String,
    pub serial: String,
    pub token: SessionToken,
    pub auth: String,
    pub commands: mpsc::Receiver<Result<HubCommand, Status>>,
}

impl FirmwareRouteFixture {
    pub async fn new(slug: &str) -> Self {
        Self::with_capability(slug, true).await
    }

    pub async fn with_capability(slug: &str, capable: bool) -> Self {
        let state = state().await;
        let tenant = state
            .tenants()
            .create(slug, "Firmware Route")
            .await
            .unwrap();
        let auth = plugin_studio_tenant_token(&state, &tenant.id.to_string(), slug).await;
        let agent_id = feature_advertisement_printer(&state, tenant.id, "agent", slug).await;
        let printer = state
            .printers()
            .list_with_live_status_for_tenant(tenant.id)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .printer;
        let token = SessionToken::new();
        state
            .agents()
            .claim_online_session(
                tenant.id,
                agent_id,
                &token.persisted_id(),
                "task-6",
                "2026-07-12T00:00:00Z",
            )
            .await
            .unwrap();
        let (command_sender, commands) = mpsc::channel(16);
        state
            .sessions()
            .register(AgentSession {
                token,
                tenant_id: tenant.id,
                agent_id,
                name: "firmware agent".to_owned(),
                version: "task-6".to_owned(),
                connected_at: "2026-07-12T00:00:00Z".to_owned(),
                last_heartbeat_at: "2026-07-12T00:00:00Z".to_owned(),
                wake_sender: mpsc::channel(1).0,
                close_sender: mpsc::channel(1).0,
                command_sender,
                capabilities: capable
                    .then_some(AgentCapability::FirmwareControl)
                    .into_iter()
                    .collect::<HashSet<_>>(),
                pending_live_commands: empty_pending_live_commands(),
                live_command_transition: Arc::new(tokio::sync::Mutex::new(())),
            })
            .await;
        state
            .printers()
            .establish_generation_if_current(
                tenant.id,
                agent_id,
                &token.persisted_id(),
                &printer.serial_number,
                GENERATION,
            )
            .await
            .unwrap();
        Self {
            state,
            tenant_id: tenant.id,
            agent_id,
            printer_id: printer.id,
            serial: printer.serial_number,
            token,
            auth,
            commands,
        }
    }

    pub fn app(&self) -> Router {
        router(self.state.clone())
    }

    pub fn uri(&self, suffix: &str) -> String {
        format!(
            "/api/v1/plugin/printers/{}/firmware{suffix}",
            self.printer_id
        )
    }

    pub fn spawn_json(
        &self,
        method: Method,
        suffix: &'static str,
        body: Value,
    ) -> tokio::task::JoinHandle<(StatusCode, Value)> {
        let app = self.app();
        let uri = self.uri(suffix);
        let auth = self.auth.clone();
        tokio::spawn(async move { request_as(app, method, &uri, Some(body), &auth).await })
    }

    pub async fn next_command(&mut self) -> HubCommand {
        tokio::time::timeout(Duration::from_millis(500), self.commands.recv())
            .await
            .expect("firmware route did not dispatch")
            .expect("firmware command channel closed")
            .expect("firmware command transport failed")
    }

    pub async fn event(&self, event: agent_event::Event) {
        self.event_result(event).await.unwrap();
    }

    pub async fn event_result(&self, event: agent_event::Event) -> Result<(), Status> {
        crate::grpc::handle_event_for_tests(
            &self.state,
            self.tenant_id,
            self.agent_id,
            self.token,
            AgentEvent {
                tenant_id: self.tenant_id.to_string(),
                agent_id: self.agent_id.to_string(),
                event_id: uuid::Uuid::new_v4().to_string(),
                event: Some(event),
            },
        )
        .await
    }

    pub async fn prepared(&self, command_id: CommandId) {
        self.event(agent_event::Event::FirmwarePrepared(FirmwarePrepared {
            command_id: command_id.to_string(),
            serial: self.serial.clone(),
            generation: GENERATION,
        }))
        .await;
    }

    pub async fn invalidated(&self, generation: u64) {
        self.event(agent_event::Event::PrinterFirmwareInvalidated(
            PrinterFirmwareInvalidated {
                serial: self.serial.clone(),
                generation,
            },
        ))
        .await;
    }

    pub async fn refreshed(
        &self,
        command_id: CommandId,
        module_revision: u64,
        modules: Vec<ProtoPrinterFirmwareModule>,
    ) {
        self.event(control_result(
            command_id,
            &self.serial,
            GENERATION,
            firmware_command_result::Outcome::RefreshedModules(FirmwareRefreshedModules {
                modules,
                module_revision,
            }),
        ))
        .await;
    }

    pub async fn published(&self, command_id: CommandId) {
        self.event(agent_event::Event::FirmwarePublished(FirmwarePublished {
            command_id: command_id.to_string(),
            serial: self.serial.clone(),
            generation: GENERATION,
        }))
        .await;
    }

    pub async fn acknowledged(&self, command_id: CommandId, command: &str, sequence_id: &str) {
        self.event(control_result(
            command_id,
            &self.serial,
            GENERATION,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: command.to_owned(),
                sequence_id: sequence_id.to_owned(),
                result: Some("success".to_owned()),
                error_code: Some(0),
                reason: None,
                message: None,
            }),
        ))
        .await;
    }

    pub async fn typed_result(
        &self,
        command_id: CommandId,
        transient_status: Option<ProtoPrinterFirmwareStatus>,
        outcome: firmware_command_result::Outcome,
    ) -> Result<(), Status> {
        self.event_result(control_result_with_status(
            command_id,
            &self.serial,
            GENERATION,
            transient_status,
            outcome,
        ))
        .await
    }

    pub async fn acknowledgement_result(
        &self,
        command_id: CommandId,
        sequence_id: &str,
    ) -> Result<(), Status> {
        self.event_result(control_result(
            command_id,
            &self.serial,
            GENERATION,
            firmware_command_result::Outcome::Acknowledgement(FirmwareAcknowledgement {
                command: "upgrade_confirm".to_owned(),
                sequence_id: sequence_id.to_owned(),
                result: Some("success".to_owned()),
                error_code: Some(0),
                reason: None,
                message: None,
            }),
        ))
        .await
    }

    pub async fn generic_failure(&self, command_id: CommandId, error: &str) {
        self.event(agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_string(),
            success: false,
            error: error.to_owned(),
            result_json: String::new(),
            firmware_result: None,
        }))
        .await;
    }

    pub async fn prepare(&mut self, body: Value) -> (Value, HubCommand) {
        let request = self.spawn_json(Method::POST, "/prepare", body);
        let outbound = self.next_command().await;
        let command_id = CommandId::parse(&outbound.command_id).unwrap();
        self.prepared(command_id).await;
        let (status, body) = request.await.unwrap();
        assert_eq!(status, StatusCode::OK);
        (body, outbound)
    }
}

pub(super) fn upgrade_metadata(sequence_id: &str) -> Value {
    serde_json::json!({
        "command": "upgrade_confirm",
        "sequence_id": sequence_id,
        "src_id": 1
    })
}

pub(super) fn upgrade_command(sequence_id: &str) -> Value {
    upgrade_metadata(sequence_id)
}

pub(super) fn start_metadata(sequence_id: &str, module: &str, version: &str) -> Value {
    serde_json::json!({
        "command": "start",
        "sequence_id": sequence_id,
        "src_id": 1,
        "module": module,
        "version": version
    })
}

pub(super) fn start_command(sequence_id: &str, url: &str, module: &str, version: &str) -> Value {
    serde_json::json!({
        "command": "start",
        "sequence_id": sequence_id,
        "src_id": 1,
        "url": url,
        "module": module,
        "version": version
    })
}

pub(super) fn module(name: &str, version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: name.to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

pub(super) fn proto_module(name: &str, version: &str) -> ProtoPrinterFirmwareModule {
    ProtoPrinterFirmwareModule {
        name: name.to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

pub(super) async fn raw_json_status(
    app: Router,
    method: Method,
    uri: &str,
    auth: Option<&str>,
    body: String,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(auth) = auth {
        builder = builder.header(AUTHORIZATION, format!("Bearer {auth}"));
    }
    let response = app
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

pub(super) fn command_id(outbound: &HubCommand) -> CommandId {
    CommandId::parse(&outbound.command_id).unwrap()
}
