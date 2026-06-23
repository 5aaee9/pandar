use axum::{
    Json, body::Bytes, extract::Path, extract::State, extract::rejection::JsonRejection,
    http::HeaderMap,
};
use pandar_core::{AgentId, CommandId, CommandRecord, Printer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    AppState,
    repositories::{DiagnosePrinterPayload, DiscoverPrintersPayload, MaterialSnapshot, UserRole},
    routes::{ApiError, auth},
};

const DEFAULT_DISCOVERY_TIMEOUT_SECONDS: u32 = 5;
const MIN_DISCOVERY_TIMEOUT_SECONDS: u32 = 1;
const MAX_DISCOVERY_TIMEOUT_SECONDS: u32 = 15;

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
    materials: Option<PrinterMaterialsResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct PrinterMaterialsResponse {
    ams_units: Value,
    external_spools: Value,
    active_tray: Option<Value>,
    observed_at: String,
}

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
        .map(|snapshot| {
            (
                snapshot.printer_id.clone(),
                PrinterMaterialsResponse::from(snapshot),
            )
        })
        .collect::<HashMap<_, _>>();
    let printers = state
        .printers()
        .list_for_tenant(tenant_id)
        .await?
        .into_iter()
        .map(|printer| {
            let materials = materials.get(&printer.id).cloned();
            PrinterResponse::from_parts(printer, materials)
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
        .await?
        .map(PrinterMaterialsResponse::from);

    Ok(Json(PrinterResponse::from_parts(printer, materials)))
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
    state.sessions().wake_agent(tenant_id, agent_id).await;

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
    state.sessions().wake_agent(tenant_id, agent_id).await;

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
    state.sessions().wake_agent(tenant_id, agent_id).await;

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

impl PrinterResponse {
    pub(in crate::routes) fn from_parts(
        printer: Printer,
        materials: Option<PrinterMaterialsResponse>,
    ) -> Self {
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
            materials,
        }
    }
}

impl From<MaterialSnapshot> for PrinterMaterialsResponse {
    fn from(snapshot: MaterialSnapshot) -> Self {
        Self {
            ams_units: scrub_material_json(snapshot.ams_units),
            external_spools: scrub_material_json(snapshot.external_spools),
            active_tray: snapshot.active_tray.map(scrub_material_json),
            observed_at: snapshot.observed_at,
        }
    }
}

fn scrub_material_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(scrub_material_json).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    (!credential_key(&key)).then(|| (key, scrub_material_json(value)))
                })
                .collect(),
        ),
        value => value,
    }
}

fn credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["access_code", "password", "passwd", "token", "auth"]
        .iter()
        .any(|needle| key.contains(needle))
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
