use anyhow::Context;
use tokio::sync::mpsc;

mod artifacts;
mod config;
mod diagnostics;
mod events;
mod operations;
#[cfg(test)]
pub(crate) use artifacts::resolve_artifact_path;
pub use artifacts::{
    ArtifactReader, FilesystemArtifactReader, HubArtifactReader, artifact_download_url,
};
use artifacts::{CommandArtifactReader, LegacyCommandArtifactReader, PrintCommandArtifactReader};
pub use config::parse_printer_config;
use events::event;

use crate::{
    AgentConfig,
    machine::{BambuMachineGateway, MachineSnapshot},
    protocol::agent::v1::{
        AgentEvent, CommandAck, CommandResult, PrintProjectFile, PrinterSnapshot, agent_event,
        hub_command,
    },
};

pub async fn handle_command_with_gateway<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command: crate::protocol::agent::v1::HubCommand,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    match command.command {
        Some(hub_command::Command::PrintProjectFile(print)) => {
            emit_print_project_file_events(config, gateway, sender, &command.command_id, print)
                .await
        }
        other => {
            handle_command_with_reader(
                config,
                gateway,
                &FilesystemArtifactReader::new(config.artifact_root.clone()),
                sender,
                crate::protocol::agent::v1::HubCommand {
                    command_id: command.command_id,
                    command: other,
                },
            )
            .await
        }
    }
}

pub async fn handle_command_with_reader<G, R>(
    config: &AgentConfig,
    gateway: &G,
    artifact_reader: &R,
    sender: &mpsc::Sender<AgentEvent>,
    command: crate::protocol::agent::v1::HubCommand,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    R: ArtifactReader,
{
    match command.command {
        Some(hub_command::Command::RefreshPrinters(_)) => {
            emit_refresh_printers_events(config, gateway, sender, &command.command_id).await
        }
        Some(hub_command::Command::PrintProjectFile(print)) => {
            emit_print_project_file_events_with_reader(
                config,
                gateway,
                artifact_reader,
                sender,
                &command.command_id,
                print,
            )
            .await
        }
        Some(hub_command::Command::DiscoverPrinters(discovery)) => {
            diagnostics::emit_discover_events(
                config,
                gateway,
                sender,
                &command.command_id,
                discovery,
            )
            .await
        }
        Some(hub_command::Command::DiagnosePrinter(diagnostic)) => {
            diagnostics::emit_diagnose_events(
                config,
                gateway,
                sender,
                &command.command_id,
                diagnostic,
            )
            .await
        }
        Some(hub_command::Command::PrinterOperation(operation)) => {
            operations::emit_events(config, gateway, sender, &command.command_id, operation).await
        }
        None => Ok(()),
    }
}

pub fn ack_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    command_ack_event(config, command_id, true, String::new())
}

fn rejected_ack_event(config: &AgentConfig, command_id: &str, error: String) -> AgentEvent {
    command_ack_event(config, command_id, false, error)
}

fn command_ack_event(
    config: &AgentConfig,
    command_id: &str,
    accepted: bool,
    error: String,
) -> AgentEvent {
    event(
        config,
        "ack",
        agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_owned(),
            accepted,
            error,
        }),
    )
}

pub fn success_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    result_event(config, command_id, true, String::new(), String::new())
}

fn failure_event(config: &AgentConfig, command_id: &str, error: String) -> AgentEvent {
    result_event(config, command_id, false, error, String::new())
}

fn success_event_with_result(
    config: &AgentConfig,
    command_id: &str,
    result_json: String,
) -> AgentEvent {
    result_event(config, command_id, true, String::new(), result_json)
}

fn result_event(
    config: &AgentConfig,
    command_id: &str,
    success: bool,
    error: String,
    result_json: String,
) -> AgentEvent {
    event(
        config,
        if success { "success" } else { "failure" },
        agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_owned(),
            success,
            error,
            result_json,
        }),
    )
}

fn printer_snapshot_event(config: &AgentConfig, snapshot: MachineSnapshot) -> AgentEvent {
    event(
        config,
        "printer-snapshot",
        agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: snapshot.serial,
            name: snapshot.name,
            state: snapshot.state,
            model: snapshot.model.unwrap_or_default(),
        }),
    )
}

async fn emit_refresh_printers_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue refresh-printers command ack")?;

    match gateway.refresh_printers().await {
        Ok(snapshots) => {
            for snapshot in snapshots {
                sender
                    .send(printer_snapshot_event(config, snapshot))
                    .await
                    .context("queue printer snapshot event")?;
            }
            sender
                .send(success_event(config, command_id))
                .await
                .context("queue refresh-printers command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue refresh-printers command failure")?;
        }
    }

    Ok(())
}

async fn emit_print_project_file_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: PrintProjectFile,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    let artifact_reader = CommandArtifactReader::new(config);
    emit_print_project_file_events_with_command_reader(
        config,
        gateway,
        &artifact_reader,
        sender,
        command_id,
        command,
    )
    .await
}

async fn emit_print_project_file_events_with_reader<G, R>(
    config: &AgentConfig,
    gateway: &G,
    artifact_reader: &R,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: PrintProjectFile,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    R: ArtifactReader,
{
    emit_print_project_file_events_with_command_reader(
        config,
        gateway,
        &LegacyCommandArtifactReader { artifact_reader },
        sender,
        command_id,
        command,
    )
    .await
}

async fn emit_print_project_file_events_with_command_reader<G, R>(
    config: &AgentConfig,
    gateway: &G,
    artifact_reader: &R,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: PrintProjectFile,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    R: PrintCommandArtifactReader,
{
    if let Err(err) = gateway.validate_printer(&command.serial_number).await {
        let error = gateway.redact_error(&format!("{err:#}"));
        sender
            .send(rejected_ack_event(config, command_id, error))
            .await
            .context("queue print-project-file rejected ack")?;
        return Ok(());
    }

    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue print-project-file command ack")?;

    let result = async {
        let artifact = artifact_reader
            .read_print_artifact(&command)
            .await
            .with_context(|| read_print_artifact_context(&command))?;
        gateway
            .print_project_file(&command.serial_number, &command, artifact)
            .await
            .with_context(|| format!("dispatch print job {}", command.job_id))
    }
    .await;

    match result {
        Ok(()) => {
            sender
                .send(success_event(config, command_id))
                .await
                .context("queue print-project-file command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue print-project-file command failure")?;
        }
    }

    Ok(())
}

fn read_print_artifact_context(command: &PrintProjectFile) -> String {
    if command.artifact_download_path.trim().is_empty() {
        format!("read print artifact {}", command.storage_path)
    } else {
        "read print artifact from hub".to_string()
    }
}

#[cfg(test)]
mod tests;
