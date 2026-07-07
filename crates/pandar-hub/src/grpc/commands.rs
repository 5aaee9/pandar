use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use tonic::Status;

mod conversion;
pub use conversion::{
    CommandConversionOptions, hub_command_from_record, hub_command_from_record_with_options,
};

use crate::{
    AppState,
    protocol::agent::v1::{CommandResult, HubCommand},
    repositories::RepositoryError,
};

pub async fn mark_sent_and_job(
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

pub async fn next_hub_command_for_agent(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
) -> Result<Option<HubCommand>, Status> {
    next_hub_command_for_agent_with_options(
        state,
        tenant_id,
        agent_id,
        CommandConversionOptions {
            require_artifact_download_path: state.artifact_storage().backend().requires_hub_fetch(),
        },
    )
    .await
}

pub async fn next_hub_command_for_agent_with_options(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    options: CommandConversionOptions,
) -> Result<Option<HubCommand>, Status> {
    let Some(command) = state
        .commands()
        .next_queued_for_agent(tenant_id, agent_id)
        .await
        .map_err(repository_status)?
    else {
        return Ok(None);
    };

    let hub_command = hub_command_from_record_with_options(command.clone(), options)?;
    mark_sent_and_job(state, command, tenant_id, agent_id).await?;
    Ok(Some(hub_command))
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
    let result_json = result.result_json;
    if success {
        if command.kind == "print_project_file" {
            state
                .jobs()
                .mark_print_succeeded_with_result(
                    command_id,
                    tenant_id,
                    agent_id,
                    optional_result_json(&command.kind, result_json, link_printer_access_code),
                )
                .await
                .map_err(repository_status)?;
            Ok(None)
        } else {
            let command = state
                .commands()
                .mark_succeeded_with_result(
                    command_id,
                    tenant_id,
                    agent_id,
                    optional_result_json(&command.kind, result_json, link_printer_access_code),
                )
                .await
                .map_err(repository_status)?;
            Ok(Some(command))
        }
    } else {
        if command.kind == "print_project_file" {
            state
                .jobs()
                .mark_print_failed(command_id, tenant_id, agent_id, error)
                .await
                .map_err(repository_status)?;
            Ok(None)
        } else {
            let command = state
                .commands()
                .mark_failed_with_result(
                    command_id,
                    tenant_id,
                    agent_id,
                    error,
                    optional_result_json(&command.kind, result_json, link_printer_access_code),
                )
                .await
                .map_err(repository_status)?;
            Ok(Some(command))
        }
    }
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
