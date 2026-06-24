use pandar_core::{AgentId, CommandId, CommandRecord, TenantId};
use tonic::Status;

use crate::{
    AppState,
    protocol::agent::v1::{
        DiagnosePrinter, DiscoverPrinters, HubCommand, PrintProjectFile, PrinterControl,
        RefreshPrinters, hub_command,
    },
    repositories::{
        DiagnosePrinterPayload, DiscoverPrintersPayload, PrintProjectFilePayload,
        PrinterControlPayload, RepositoryError,
    },
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
    result_json: String,
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
                .mark_succeeded_with_result(
                    command_id,
                    tenant_id,
                    agent_id,
                    optional_result_json(result_json),
                )
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
                .mark_failed_with_result(
                    command_id,
                    tenant_id,
                    agent_id,
                    error,
                    optional_result_json(result_json),
                )
                .await
                .map_err(repository_status)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandConversionOptions {
    pub require_artifact_download_path: bool,
}

pub fn hub_command_from_record(command: CommandRecord) -> Result<HubCommand, Status> {
    hub_command_from_record_with_options(
        command,
        CommandConversionOptions {
            require_artifact_download_path: false,
        },
    )
}

pub fn hub_command_from_record_with_options(
    command: CommandRecord,
    options: CommandConversionOptions,
) -> Result<HubCommand, Status> {
    let command_id = command.id.to_string();
    let command = match command.kind.as_str() {
        "refresh_printers" => hub_command::Command::RefreshPrinters(RefreshPrinters {}),
        "discover_printers" => {
            let payload: DiscoverPrintersPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize discover printers command payload"
                    );
                    Status::internal("invalid discover printers command payload")
                })?;
            hub_command::Command::DiscoverPrinters(DiscoverPrinters {
                timeout_seconds: payload.timeout_seconds,
            })
        }
        "diagnose_printer" => {
            let payload: DiagnosePrinterPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize diagnose printer command payload"
                    );
                    Status::internal("invalid diagnose printer command payload")
                })?;
            hub_command::Command::DiagnosePrinter(DiagnosePrinter {
                serial_number: payload.serial_number,
            })
        }
        "printer_control" => {
            let payload: PrinterControlPayload = serde_json::from_str(&command.payload_json)
                .map_err(|err| {
                    tracing::error!(
                        command_id = %command.id,
                        error = %format!("{err:#}"),
                        "failed to deserialize printer control command payload"
                    );
                    Status::internal("invalid printer control command payload")
                })?;
            hub_command::Command::PrinterControl(PrinterControl {
                serial_number: payload.serial_number,
                action: payload.action.as_str().to_string(),
                speed_mode: payload.speed_mode.unwrap_or_default().into(),
            })
        }
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
            if options.require_artifact_download_path
                && payload.artifact_download_path.trim().is_empty()
            {
                return Err(Status::internal("missing artifact download path"));
            }
            hub_command::Command::PrintProjectFile(PrintProjectFile {
                job_id: payload.job_id,
                artifact_id: payload.artifact_id,
                printer_id: payload.printer_id,
                serial_number: payload.serial_number,
                filename: payload.filename,
                storage_path: payload.storage_path,
                artifact_download_path: payload.artifact_download_path,
                size_bytes: payload.size_bytes,
                plate_id: payload.plate_id,
                use_ams: payload.use_ams,
                flow_cali: payload.flow_cali,
                timelapse: payload.timelapse,
                ams_mapping_json: mapping_payload_string(
                    payload.ams_mapping_json.as_deref(),
                    "ams_mapping_json",
                    &command_id,
                )?,
                ams_mapping2_json: mapping_payload_string(
                    payload.ams_mapping2_json.as_deref(),
                    "ams_mapping2_json",
                    &command_id,
                )?,
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

fn mapping_payload_string(
    value: Option<&str>,
    field: &'static str,
    command_id: &str,
) -> Result<String, Status> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    match field {
        "ams_mapping_json" => {
            parse_mapping::<Vec<i32>>(value, field, command_id)?;
        }
        "ams_mapping2_json" => {
            let entries = parse_mapping::<Vec<Mapping2Payload>>(value, field, command_id)?;
            for entry in entries {
                let _ = (entry.ams_id, entry.slot_id);
            }
        }
        _ => unreachable!("print mapping field should be known"),
    }
    Ok(value.to_string())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping2Payload {
    ams_id: i32,
    slot_id: i32,
}

fn parse_mapping<T: serde::de::DeserializeOwned>(
    value: &str,
    field: &'static str,
    command_id: &str,
) -> Result<T, Status> {
    serde_json::from_str::<T>(value).map_err(|err| {
        let err = anyhow::Error::from(err).context(format!(
            "failed to parse persisted {field} for print command"
        ));
        tracing::error!(
            %command_id,
            %field,
            error = %format!("{err:#}"),
            "failed to serialize print command mapping"
        );
        Status::internal("invalid print command mapping payload")
    })
}

fn optional_result_json(result_json: String) -> Option<String> {
    (!result_json.is_empty()).then_some(result_json)
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
