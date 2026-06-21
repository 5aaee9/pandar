use axum::{Json, extract::Path, extract::State};
use pandar_core::{AgentId, CommandRecord, Printer};
use serde::Serialize;

use crate::{AppState, routes::ApiError};

#[derive(Debug, Serialize)]
pub(super) struct PrinterResponse {
    id: String,
    tenant_id: String,
    agent_id: String,
    serial_number: String,
    name: String,
    model: Option<String>,
    status: String,
    last_seen_at: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct PrinterListResponse {
    printers: Vec<PrinterResponse>,
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
    created_at: String,
    updated_at: String,
}

pub(super) async fn list_printers(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
) -> Result<Json<PrinterListResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let printers = state
        .printers()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(PrinterResponse::from)
        .collect();

    Ok(Json(PrinterListResponse { printers }))
}

pub(super) async fn get_printer(
    State(state): State<AppState>,
    Path((tenant_id, printer_id)): Path<(String, String)>,
) -> Result<Json<PrinterResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let printer_id = parse_printer_id(&printer_id)?;
    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
    else {
        return Err(ApiError::not_found("printer_not_found"));
    };

    Ok(Json(PrinterResponse::from(printer)))
}

pub(super) async fn refresh_printers(
    State(state): State<AppState>,
    Path((tenant_id, agent_id)): Path<(String, String)>,
) -> Result<Json<CommandResponse>, ApiError> {
    let tenant_id = super::parse_tenant_id(&tenant_id)?;
    let agent_id = parse_agent_id(&agent_id)?;
    let command = state
        .sessions()
        .dispatch_refresh_printers(tenant_id, agent_id, state.commands())
        .await?;

    Ok(Json(CommandResponse::from(command)))
}

fn parse_agent_id(value: &str) -> Result<AgentId, ApiError> {
    AgentId::parse(value).map_err(|_| ApiError::bad_request("invalid_agent_id"))
}

fn parse_printer_id(value: &str) -> Result<&str, ApiError> {
    uuid::Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid_printer_id"))?;
    Ok(value)
}

impl From<Printer> for PrinterResponse {
    fn from(printer: Printer) -> Self {
        Self {
            id: printer.id,
            tenant_id: printer.tenant_id.to_string(),
            agent_id: printer.agent_id.to_string(),
            serial_number: printer.serial_number,
            name: printer.name,
            model: printer.model,
            status: printer.status,
            last_seen_at: printer.last_seen_at,
            created_at: printer.created_at,
        }
    }
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
            created_at: command.created_at,
            updated_at: command.updated_at,
        }
    }
}
