use std::time::Duration;

use axum::{
    Json,
    extract::rejection::JsonRejection,
    extract::{Path, State},
    http::HeaderMap,
};
use pandar_core::compatibility::normalize_model;
use pandar_core::{
    CommandStatus, H2cAutoNozzleMappingRequest, H2cAutoNozzleMappingResponseEnvelope,
};
use serde::Deserialize;

use crate::{
    AppState,
    protocol::agent::v1::AgentCapability,
    repositories::{PrinterOperationKind, RepositoryError},
    routes::{ApiError, auth, printer_operations::live},
};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(7);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Deserialize)]
struct OperationResult {
    #[serde(rename = "type")]
    kind: String,
    action: String,
    mqtt_report: Option<H2cAutoNozzleMappingResponseEnvelope>,
}

pub(crate) async fn get_auto_nozzle_mapping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
    payload: Result<Json<H2cAutoNozzleMappingRequest>, JsonRejection>,
) -> Result<Json<H2cAutoNozzleMappingResponseEnvelope>, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let Json(request) = payload.map_err(|_| ApiError::bad_request("invalid_printer_control"))?;
    if !request.is_valid() {
        return Err(ApiError::bad_request("invalid_printer_control"));
    }
    let tenant_id = authenticated.token.tenant_id;
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await?
        .ok_or_else(|| ApiError::not_found("printer_not_found"))?;
    if printer
        .model
        .as_deref()
        .and_then(normalize_model)
        .as_deref()
        != Some("H2C")
    {
        return Err(unavailable());
    }
    let Some(token) = state
        .sessions()
        .current_token_for_capability(
            tenant_id,
            printer.agent_id,
            AgentCapability::H2cAutoNozzleMapping,
        )
        .await
    else {
        return Err(unavailable());
    };
    let rack_current = printer.bambu_nozzle_system_session_id.as_deref()
        == Some(token.persisted_id().as_str())
        && printer
            .bambu_nozzle_system
            .as_ref()
            .is_some_and(|system| system.nozzle.info.iter().any(|nozzle| nozzle.id >= 16));
    if !rack_current {
        return Err(unavailable());
    }

    let command = live::dispatch_for_printer_with_token(
        &state,
        tenant_id,
        printer,
        PrinterOperationKind::GetAutoNozzleMapping {
            request: request.clone(),
        },
        auth::plugin_audit_actor(&authenticated),
        token,
        AgentCapability::H2cAutoNozzleMapping,
    )
    .await?;

    let response = match tokio::time::timeout(RESPONSE_TIMEOUT, async {
        loop {
            let current = state
                .commands()
                .get_for_tenant(tenant_id, command.id)
                .await?
                .ok_or_else(|| ApiError::not_found("command_not_found"))?;
            if matches!(
                current.status,
                CommandStatus::Succeeded | CommandStatus::Failed | CommandStatus::Cancelled
            ) {
                return correlated_response(&current, &request);
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    })
    .await
    {
        Ok(response) => response?,
        Err(_) => terminal_timeout_response(&state, &command, &request).await?,
    };

    Ok(Json(response))
}

async fn terminal_timeout_response(
    state: &AppState,
    command: &pandar_core::CommandRecord,
    request: &H2cAutoNozzleMappingRequest,
) -> Result<H2cAutoNozzleMappingResponseEnvelope, ApiError> {
    let terminal = match state
        .commands()
        .mark_failed(
            command.id,
            command.tenant_id,
            command.agent_id,
            "H2C auto nozzle mapping response timed out",
        )
        .await
    {
        Ok(command) => command,
        Err(RepositoryError::InvalidCommandTransition { .. }) => state
            .commands()
            .get_for_tenant(command.tenant_id, command.id)
            .await?
            .ok_or_else(|| ApiError::not_found("command_not_found"))?,
        Err(error) => return Err(error.into()),
    };
    correlated_response(&terminal, request)
}

fn correlated_response(
    command: &pandar_core::CommandRecord,
    request: &H2cAutoNozzleMappingRequest,
) -> Result<H2cAutoNozzleMappingResponseEnvelope, ApiError> {
    let Some(result_json) = command.result_json.as_deref() else {
        return Err(unavailable());
    };
    let result = serde_json::from_str::<OperationResult>(result_json).map_err(|error| {
        tracing::error!(
            command_id = %command.id,
            error = %format!("{error:#}"),
            "failed to decode H2C auto nozzle mapping command result"
        );
        unavailable()
    })?;
    let Some(response) = result.mqtt_report else {
        return Err(unavailable());
    };
    if result.kind != "printer_operation"
        || result.action != "get_auto_nozzle_mapping"
        || !response.is_valid_for(request)
    {
        return Err(unavailable());
    }
    Ok(response)
}

fn unavailable() -> ApiError {
    ApiError::bad_request("h2c_auto_nozzle_mapping_unavailable")
}

#[cfg(test)]
mod tests {
    use pandar_core::{AgentId, CommandId, CommandRecord, CommandStatus, TenantId};
    use serde_json::json;

    use super::*;

    fn request() -> H2cAutoNozzleMappingRequest {
        serde_json::from_value(json!({
            "command": "get_auto_nozzle_mapping",
            "sequence_id": "42",
            "version": 1,
            "group_info": [{
                "id": 0,
                "ext": 1,
                "dia": 0.4,
                "vol": "E3D High Flow"
            }]
        }))
        .unwrap()
    }

    fn command(status: CommandStatus, mqtt_report: serde_json::Value) -> CommandRecord {
        CommandRecord {
            id: CommandId::new(),
            tenant_id: TenantId::new(),
            agent_id: AgentId::new(),
            printer_id: Some("printer".to_owned()),
            kind: "printer_operation".to_owned(),
            status,
            payload_json: "{}".to_owned(),
            result_json: Some(
                json!({
                    "type": "printer_operation",
                    "action": "get_auto_nozzle_mapping",
                    "mqtt_report": mqtt_report
                })
                .to_string(),
            ),
            error: Some("printer reported rack busy".to_owned()),
            created_at: "2026-08-01T00:00:00Z".to_owned(),
            updated_at: "2026-08-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn failed_command_returns_correlated_printer_failure_detail() {
        let command = command(
            CommandStatus::Failed,
            json!({
                "print": {
                    "command": "get_auto_nozzle_mapping",
                    "sequence_id": "42",
                    "result": "fail",
                    "version": "future",
                    "reason": "rack busy",
                    "errno": 17
                }
            }),
        );

        let response = correlated_response(&command, &request()).unwrap();
        assert_eq!(response.print.reason.as_deref(), Some("rack busy"));
        assert_eq!(response.print.errno, Some(17));
    }

    #[test]
    fn successful_command_rejects_a_mismatched_mapping_version() {
        let command = command(
            CommandStatus::Succeeded,
            json!({
                "print": {
                    "command": "get_auto_nozzle_mapping",
                    "sequence_id": "42",
                    "result": "success",
                    "version": 0,
                    "mapping": [16]
                }
            }),
        );

        assert!(correlated_response(&command, &request()).is_err());
    }
}
