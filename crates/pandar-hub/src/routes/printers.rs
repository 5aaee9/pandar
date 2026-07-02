use axum::{
    Json,
    body::Bytes,
    extract::Path,
    extract::State,
    extract::rejection::JsonRejection,
    http::{HeaderMap, StatusCode},
};
use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future, net::Ipv4Addr};

use crate::{
    AppState,
    printer_events::{PrinterEventPrinter, printer_event_printer},
    protocol::agent::v1::{HubCommand, LinkPrinter, hub_command},
    repositories::{
        DiagnosePrinterPayload, DiscoverPrintersPayload, LinkPrinterPayload, RepositoryResult,
        UserRole,
    },
    routes::{ApiError, auth, printer_operations::PrinterOperationRequest},
    sessions::LiveDispatchError,
};

const DEFAULT_DISCOVERY_TIMEOUT_SECONDS: u32 = 5;
const MIN_DISCOVERY_TIMEOUT_SECONDS: u32 = 1;
const MAX_DISCOVERY_TIMEOUT_SECONDS: u32 = 15;

pub(super) type PrinterResponse = PrinterEventPrinter;
#[derive(Debug, Serialize)]
pub(super) struct PrinterListResponse {
    pub(in crate::routes) printers: Vec<PrinterResponse>,
}

#[derive(Debug, Serialize)]
pub(super) struct CommandResponse {
    id: String,
    tenant_id: String,
    agent_id: String,
    printer_id: Option<String>,
    kind: String,
    status: String,
    payload_json: String,
    error: Option<String>,
    result_json: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct DiscoverPrintersRequest {
    timeout_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiagnosePrinterRequest {
    serial_number: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkPrinterRequest {
    #[serde(rename = "type")]
    printer_type: String,
    host: String,
    access_code: String,
    name: Option<String>,
}

pub(super) async fn list_printers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> Result<Json<PrinterListResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let materials = state
        .materials()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|snapshot| (snapshot.printer_id.clone(), snapshot))
        .collect::<HashMap<_, _>>();
    let printers = state
        .printers()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|printer| {
            let materials = materials.get(&printer.id).cloned();
            printer_event_printer(printer, materials)
        })
        .collect();

    Ok(Json(PrinterListResponse { printers }))
}

pub(super) async fn get_printer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
) -> Result<Json<PrinterResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let printer_id = parse_printer_id(&printer_id)?;
    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
    else {
        return Err(ApiError::not_found("printer_not_found"));
    };
    let materials = state
        .materials()
        .latest_for_printer(tenant_id, printer_id)
        .await?;

    Ok(Json(printer_event_printer(printer, materials)))
}

pub(super) async fn refresh_printers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let agent_id = parse_agent_id(&agent_id)?;
    let command = state
        .commands()
        .enqueue_refresh_printers_with_audit(tenant_id, agent_id, auth::audit_actor(&auth))
        .await?;
    state.wake_agent(tenant_id, agent_id).await;

    Ok(Json(CommandResponse::from(command)))
}

pub(super) async fn refresh_printer_materials(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let printer_id = parse_printer_id(&printer_id)?;
    let command = state
        .commands()
        .enqueue_refresh_printer_materials_with_audit(
            tenant_id,
            printer_id,
            auth::audit_actor(&auth),
        )
        .await?;
    state.wake_agent(tenant_id, command.agent_id).await;

    Ok(Json(CommandResponse::from(command)))
}

pub(super) async fn discover_printers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
    payload: Bytes,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let agent_id = parse_agent_id(&agent_id)?;
    let timeout_seconds = if payload.is_empty() {
        DEFAULT_DISCOVERY_TIMEOUT_SECONDS
    } else {
        serde_json::from_slice::<DiscoverPrintersRequest>(&payload)
            .map_err(|_| ApiError::bad_request("bad_request"))?
            .timeout_seconds
            .unwrap_or(DEFAULT_DISCOVERY_TIMEOUT_SECONDS)
    };
    if !(MIN_DISCOVERY_TIMEOUT_SECONDS..=MAX_DISCOVERY_TIMEOUT_SECONDS).contains(&timeout_seconds) {
        return Err(ApiError::bad_request("invalid_discovery_timeout"));
    }

    let command = state
        .commands()
        .enqueue_discover_printers_with_audit(
            tenant_id,
            agent_id,
            DiscoverPrintersPayload { timeout_seconds },
            auth::audit_actor(&auth),
        )
        .await?;
    state.wake_agent(tenant_id, agent_id).await;

    Ok(Json(CommandResponse::from(command)))
}

pub(super) async fn diagnose_printer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
    payload: Result<Json<DiagnosePrinterRequest>, JsonRejection>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let agent_id = parse_agent_id(&agent_id)?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let command = state
        .commands()
        .enqueue_diagnose_printer_with_audit(
            tenant_id,
            agent_id,
            DiagnosePrinterPayload {
                serial_number: payload.serial_number,
            },
            auth::audit_actor(&auth),
        )
        .await?;
    state.wake_agent(tenant_id, agent_id).await;

    Ok(Json(CommandResponse::from(command)))
}

pub(super) async fn link_printer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, agent_id)): Path<(String, String)>,
    payload: Result<Json<LinkPrinterRequest>, JsonRejection>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let agent_id = parse_agent_id(&agent_id)?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;
    let payload = payload.into_payload()?;

    let Some(agent) = state.agents().get(agent_id).await? else {
        return Err(ApiError::not_found("agent_not_found"));
    };
    if agent.tenant_id != tenant_id {
        return Err(ApiError::not_found("agent_not_found"));
    }

    let Some(token) = state.sessions().current_token(tenant_id, agent_id).await else {
        return Err(ApiError::new(StatusCode::CONFLICT, "agent_not_connected"));
    };

    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            payload.clone(),
            auth::audit_actor(&auth),
        )
        .await?;
    let hub_command = link_printer_hub_command(command.id, &payload);

    match state
        .sessions()
        .try_dispatch_live_command(tenant_id, agent_id, token, command.id, hub_command)
        .await
    {
        Ok(()) => Ok(Json(CommandResponse::from(command))),
        Err(LiveDispatchError::NotCurrent) => {
            let failed = fail_link_printer_dispatch_after_commit(
                command.id,
                tenant_id,
                agent_id,
                &payload,
                "agent connection closed before printer link completed".to_owned(),
                |command_id, tenant_id, agent_id, error| async move {
                    state
                        .commands()
                        .mark_failed(command_id, tenant_id, agent_id, error)
                        .await
                },
            )
            .await?;
            Ok(Json(CommandResponse::from(failed)))
        }
        Err(LiveDispatchError::ChannelClosed | LiveDispatchError::ChannelFull) => {
            let failed = fail_link_printer_dispatch_after_commit(
                command.id,
                tenant_id,
                agent_id,
                &payload,
                "agent command channel unavailable before printer link completed".to_owned(),
                |command_id, tenant_id, agent_id, error| async move {
                    state
                        .commands()
                        .mark_failed(command_id, tenant_id, agent_id, error)
                        .await
                },
            )
            .await?;
            Ok(Json(CommandResponse::from(failed)))
        }
    }
}

pub(super) async fn printer_control(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
    payload: Result<Json<PrinterOperationRequest>, JsonRejection>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let printer_id = parse_printer_id(&printer_id)?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("invalid_printer_control"))?;
    let operation = payload.into_operation()?;
    let command = state
        .commands()
        .enqueue_printer_operation_with_audit(
            tenant_id,
            printer_id,
            operation,
            auth::audit_actor(&auth),
        )
        .await?;
    state.wake_agent(tenant_id, command.agent_id).await;

    Ok(Json(CommandResponse::from(command)))
}

pub(super) async fn get_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, command_id)): Path<(String, String)>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let command_id = parse_command_id(&command_id)?;
    let Some(command) = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await?
    else {
        return Err(ApiError::not_found("command_not_found"));
    };

    Ok(Json(CommandResponse::from(command)))
}

fn parse_agent_id(value: &str) -> Result<AgentId, ApiError> {
    AgentId::parse(value).map_err(|_| ApiError::bad_request("invalid_agent_id"))
}

fn parse_command_id(value: &str) -> Result<CommandId, ApiError> {
    CommandId::parse(value).map_err(|_| ApiError::bad_request("invalid_command_id"))
}

fn parse_printer_id(value: &str) -> Result<&str, ApiError> {
    uuid::Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid_printer_id"))?;
    Ok(value)
}

impl LinkPrinterRequest {
    fn into_payload(self) -> Result<LinkPrinterPayload, ApiError> {
        let printer_type = trim_required(self.printer_type)?;
        if printer_type != "BambuLab" {
            return Err(ApiError::bad_request("bad_request"));
        }

        let host = trim_required(self.host)?;
        host.parse::<Ipv4Addr>()
            .map_err(|_| ApiError::bad_request("bad_request"))?;

        Ok(LinkPrinterPayload {
            printer_type,
            host,
            access_code: trim_required(self.access_code)?,
            name: trim_optional(self.name),
        })
    }
}

fn trim_required(value: String) -> Result<String, ApiError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    Ok(value)
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn link_printer_hub_command(command_id: CommandId, payload: &LinkPrinterPayload) -> HubCommand {
    HubCommand {
        command_id: command_id.to_string(),
        command: Some(hub_command::Command::LinkPrinter(LinkPrinter {
            host: payload.host.clone(),
            access_code: payload.access_code.clone(),
            name: payload.name.clone().unwrap_or_default(),
            printer_type: payload.printer_type.clone(),
        })),
    }
}

async fn fail_link_printer_dispatch_after_commit<F, Fut>(
    command_id: CommandId,
    tenant_id: TenantId,
    agent_id: AgentId,
    payload: &LinkPrinterPayload,
    error: String,
    mark_failed: F,
) -> Result<CommandRecord, ApiError>
where
    F: FnOnce(CommandId, TenantId, AgentId, String) -> Fut,
    Fut: Future<Output = RepositoryResult<CommandRecord>>,
{
    mark_failed(command_id, tenant_id, agent_id, error)
        .await
        .map_err(|err| {
            let error = crate::redaction::redact_link_printer_secret(
                &format!("{err:#}"),
                &payload.access_code,
            );
            tracing::error!(
                command_id = %command_id,
                error = %error,
                "failed to mark live printer link dispatch failed after command commit"
            );
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })
}

impl From<CommandRecord> for CommandResponse {
    fn from(command: CommandRecord) -> Self {
        Self {
            id: command.id.to_string(),
            tenant_id: command.tenant_id.to_string(),
            agent_id: command.agent_id.to_string(),
            printer_id: command.printer_id,
            kind: command.kind,
            status: command.status.to_string(),
            payload_json: command.payload_json,
            error: command.error,
            result_json: command.result_json,
            created_at: command.created_at,
            updated_at: command.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{self, Write},
        sync::{Arc, Mutex},
    };
    use tracing_subscriber::fmt::MakeWriter;

    #[tokio::test]
    async fn link_printer_dispatch_failure_helper_redacts_access_code_in_logs() {
        let logs = CapturedLogs::new();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logs.writer())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let access_code = "SECRET-LINK-CODE";
        let payload = LinkPrinterPayload {
            printer_type: "BambuLab".to_owned(),
            host: "192.0.2.10".to_owned(),
            access_code: access_code.to_owned(),
            name: None,
        };

        let err = fail_link_printer_dispatch_after_commit(
            CommandId::new(),
            TenantId::new(),
            AgentId::new(),
            &payload,
            "agent connection closed before printer link completed".to_owned(),
            |_command_id, _tenant_id, _agent_id, _error| async move {
                Err(crate::repositories::RepositoryError::Database(
                    anyhow::anyhow!("failed while handling access_code=SECRET-LINK-CODE"),
                ))
            },
        )
        .await
        .unwrap_err();
        drop(_guard);

        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "internal_server_error");
        assert!(!logs.to_string().contains(access_code));
    }

    #[derive(Clone)]
    struct CapturedLogs {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl CapturedLogs {
        fn new() -> Self {
            Self {
                output: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn writer(&self) -> TestLogWriter {
            TestLogWriter {
                output: self.output.clone(),
            }
        }
    }

    impl std::fmt::Display for CapturedLogs {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let output = self.output.lock().unwrap().clone();
            formatter.write_str(&String::from_utf8_lossy(&output))
        }
    }

    #[derive(Clone)]
    struct TestLogWriter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl<'writer> MakeWriter<'writer> for TestLogWriter {
        type Writer = TestLogBuffer;

        fn make_writer(&'writer self) -> Self::Writer {
            TestLogBuffer {
                output: self.output.clone(),
            }
        }
    }

    struct TestLogBuffer {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for TestLogBuffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
