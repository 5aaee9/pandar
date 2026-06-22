use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use tonic::Status;

use crate::{
    AppState,
    protocol::agent::v1::{HubCommand, PrintProjectFile, RefreshPrinters, hub_command},
    repositories::{PrintProjectFilePayload, RepositoryError},
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

pub async fn handle_ack_and_job(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    command_id: CommandId,
    accepted: bool,
    error: String,
) -> Result<(), Status> {
    let command = state
        .commands()
        .load_owned(command_id, tenant_id, agent_id)
        .await
        .map_err(repository_status)?;
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
    success: bool,
    error: String,
) -> Result<(), Status> {
    let command = state
        .commands()
        .load_owned(command_id, tenant_id, agent_id)
        .await
        .map_err(repository_status)?;
    if success {
        if command.kind == "print_project_file" {
            state
                .jobs()
                .mark_print_succeeded(command_id, tenant_id, agent_id)
                .await
                .map_err(repository_status)?;
        } else {
            state
                .commands()
                .mark_succeeded(command_id, tenant_id, agent_id)
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

pub fn hub_command_from_record(command: CommandRecord) -> Result<HubCommand, Status> {
    let command_id = command.id.to_string();
    let command = match command.kind.as_str() {
        "refresh_printers" => hub_command::Command::RefreshPrinters(RefreshPrinters {}),
        "print_project_file" => {
            let payload: PrintProjectFilePayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize print project file command payload"
                    );
                    Status::internal("invalid print command payload")
                })?;
            hub_command::Command::PrintProjectFile(PrintProjectFile {
                job_id: payload.job_id,
                artifact_id: payload.artifact_id,
                printer_id: payload.printer_id,
                serial_number: payload.serial_number,
                filename: payload.filename,
                storage_path: payload.storage_path,
                size_bytes: payload.size_bytes,
                plate_id: payload.plate_id,
                use_ams: payload.use_ams,
                flow_cali: payload.flow_cali,
                timelapse: payload.timelapse,
            })
        }
        kind => {
            tracing::error!(%command_id, %kind, "unknown persisted command kind");
            return Err(Status::internal("unknown persisted command kind"));
        }
    };

    Ok(HubCommand {
        command_id,
        command: Some(command),
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
