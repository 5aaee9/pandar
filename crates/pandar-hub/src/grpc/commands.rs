use pandar_core::{AgentId, CommandId, CommandRecord, PrintTransferFailure, TenantId};
use tonic::Status;

mod agent_capabilities;
mod conversion;
mod device_features;
pub use conversion::{
    CommandConversionOptions, hub_command_from_record, hub_command_from_record_with_options,
    live_printer_operation_hub_command,
};
#[cfg(test)]
pub(crate) use device_features::pause as required_feature_dispatch_pause;
pub(super) use device_features::{
    SessionQueuedDispatch, dispatch_next_queued_for_session,
    finalize_required_features_for_closing_session,
};

use crate::{AppState, repositories::RepositoryError};
use pandar_protocol::agent::v1::CommandResult;

async fn mark_sent_and_job(
    state: &AppState,
    command: CommandRecord,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> Result<CommandRecord, Status> {
    if command.kind == "print_project_file" {
        return state
            .jobs()
            .mark_print_sent(command.id, tenant_id, agent_id)
            .await
            .map_err(repository_status);
    }

    state
        .commands()
        .mark_sent(command.id, tenant_id, agent_id)
        .await
        .map_err(repository_status)
}

pub async fn handle_ack_and_job(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    accepted: bool,
    error: String,
    link_printer_access_code: Option<&str>,
) -> Result<(), Status> {
    let command = state
        .commands()
        .load_owned(command_id, tenant_id, agent_id)
        .await
        .map_err(repository_status)?;
    let error = redact_command_error(&command.kind, &error, link_printer_access_code);
    if accepted {
        if command.kind == "print_project_file" {
            state
                .jobs()
                .mark_print_acknowledged(command_id, tenant_id, agent_id)
                .await
                .map_err(repository_status)?;
        } else {
            state
                .commands()
                .mark_acknowledged(command_id, tenant_id, agent_id)
                .await
                .map_err(repository_status)?;
        }
    } else {
        if command.kind == "print_project_file" {
            state
                .jobs()
                .mark_print_failed(command_id, tenant_id, agent_id, error)
                .await
                .map_err(repository_status)?;
        } else {
            state
                .commands()
                .mark_failed(command_id, tenant_id, agent_id, error)
                .await
                .map_err(repository_status)?;
        }
    }
    Ok(())
}

pub async fn handle_result_and_job(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    result: CommandResult,
    link_printer_access_code: Option<&str>,
) -> Result<Option<CommandRecord>, Status> {
    let command = state
        .commands()
        .load_owned(command_id, tenant_id, agent_id)
        .await
        .map_err(repository_status)?;
    let success = result.success;
    let error = redact_command_error(&command.kind, &result.error, link_printer_access_code);
    let result_json = if !success && command.kind == "print_project_file" {
        print_transfer_failure_result_json(result.result_json, &error)?
    } else {
        optional_result_json(&command.kind, result.result_json, link_printer_access_code)
    };
    if success {
        if command.kind == "print_project_file" {
            state
                .jobs()
                .mark_print_succeeded_with_result(command_id, tenant_id, agent_id, result_json)
                .await
                .map_err(repository_status)?;
            Ok(None)
        } else {
            let command = state
                .commands()
                .mark_succeeded_with_result(command_id, tenant_id, agent_id, result_json)
                .await
                .map_err(repository_status)?;
            Ok(Some(command))
        }
    } else if command.kind == "print_project_file" {
        state
            .jobs()
            .mark_print_failed_with_result(command_id, tenant_id, agent_id, error, result_json)
            .await
            .map_err(repository_status)?;
        Ok(None)
    } else {
        let command = state
            .commands()
            .mark_failed_with_result(command_id, tenant_id, agent_id, error, result_json)
            .await
            .map_err(repository_status)?;
        Ok(Some(command))
    }
}

fn print_transfer_failure_result_json(
    result_json: String,
    cause: &str,
) -> Result<Option<String>, Status> {
    if result_json.is_empty() {
        return Ok(None);
    }
    let mut failure = serde_json::from_str::<PrintTransferFailure>(&result_json)
        .map_err(|_| Status::invalid_argument("invalid print transfer failure result"))?;
    failure.cause = cause.to_owned();
    Ok(Some(serde_json::to_string(&failure).expect(
        "validated print transfer failure is serializable",
    )))
}

fn redact_command_error(kind: &str, error: &str, link_printer_access_code: Option<&str>) -> String {
    if kind == "link_printer" {
        if let Some(access_code) = link_printer_access_code {
            return crate::redaction::redact_link_printer_secret(error, access_code);
        }

        let redacted = crate::redaction::redact_secrets(error);
        if redacted == error && !error.is_empty() {
            return "[redacted]".to_owned();
        }
        return redacted;
    }

    crate::redaction::redact_secrets(error)
}

fn optional_result_json(
    kind: &str,
    result_json: String,
    link_printer_access_code: Option<&str>,
) -> Option<String> {
    (!result_json.is_empty()).then(|| {
        if kind == "link_printer" {
            if let Some(access_code) = link_printer_access_code {
                return crate::redaction::redact_link_printer_result_json(
                    &result_json,
                    access_code,
                );
            }

            return crate::redaction::redact_link_printer_result_json_without_secret(&result_json);
        }

        crate::redaction::redact_result_json(&result_json)
    })
}

pub fn parse_command_id(command_id: &str) -> Result<CommandId, Status> {
    CommandId::parse(command_id).map_err(|_| Status::invalid_argument("command_id must be a UUID"))
}

pub fn repository_status(err: RepositoryError) -> Status {
    match err {
        RepositoryError::MissingAgent
        | RepositoryError::MissingCommand
        | RepositoryError::MissingPrinter
        | RepositoryError::MissingJob => Status::not_found(err.to_string()),
        RepositoryError::CommandOwnershipMismatch => Status::permission_denied(err.to_string()),
        RepositoryError::InvalidCommandTransition { .. } => {
            Status::failed_precondition(err.to_string())
        }
        err => {
            tracing::error!(error = %format!("{err:#}"), "unexpected repository error");
            Status::internal("unexpected repository error")
        }
    }
}
