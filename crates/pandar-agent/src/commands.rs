use anyhow::Context;
use tokio::sync::mpsc;

mod artifacts;
mod config;
mod diagnostics;
mod events;
mod firmware;
mod link;
mod operation_results;
mod operations;
mod print_project;
mod refresh;
mod reload_connection;
mod responses;
#[cfg(test)]
pub(crate) use artifacts::resolve_artifact_path;
pub use artifacts::{
    ArtifactReader, FilesystemArtifactReader, HubArtifactReader, artifact_download_url,
};
pub use config::parse_printer_config;
pub(crate) use firmware::{handle_firmware_command, is_firmware_command};
use link::emit_link_printer_events;
use print_project::{emit_print_project_file_events, emit_print_project_file_events_with_reader};
use refresh::{emit_refresh_printer_materials_events, emit_refresh_printers_events};
use reload_connection::emit_reload_printer_connection_events;
#[cfg(test)]
pub(crate) use responses::success_event;
pub(super) use responses::{
    ack_event, authoritative_printer_snapshot_event, failure_event, failure_event_with_result,
    printer_materials_snapshot_event, rejected_ack_event, success_event_with_result,
};

use crate::{AgentConfig, machine::BambuMachineGateway};
#[cfg(test)]
pub(crate) use pandar_protocol::agent::v1::agent_event;
use pandar_protocol::agent::v1::{AgentEvent, hub_command};

/// Admission outcome for an inbound hub command. Illegal command input is a
/// protocol rejection: acknowledge it and keep the reverse stream alive. Only
/// queue or runtime failures may tear the stream down.
pub(crate) enum CommandAdmission<T> {
    Valid(T),
    ProtocolReject(String),
}

pub(crate) async fn reject_protocol_command(
    config: &AgentConfig,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    reason: String,
) -> anyhow::Result<()> {
    sender
        .send(rejected_ack_event(config, command_id, reason))
        .await
        .context("queue command protocol-rejection ack")
}

pub async fn handle_non_firmware_command_with_gateway<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command: pandar_protocol::agent::v1::HubCommand,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    let command_id = command.command_id.clone();
    match command.command {
        Some(hub_command::Command::LinkPrinter(link)) => {
            emit_link_printer_events(config, gateway, sender, &command_id, link).await
        }
        Some(hub_command::Command::ReloadPrinterConnection(reload)) => {
            emit_reload_printer_connection_events(config, gateway, sender, &command_id, reload)
                .await
        }
        Some(hub_command::Command::PrintProjectFile(print)) => {
            emit_print_project_file_events(config, gateway, sender, &command_id, print).await
        }
        other => {
            handle_command_with_reader(
                config,
                gateway,
                &FilesystemArtifactReader::new(config.artifact_root.clone()),
                sender,
                pandar_protocol::agent::v1::HubCommand {
                    command_id,
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
    command: pandar_protocol::agent::v1::HubCommand,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    R: ArtifactReader,
{
    match command.command {
        Some(hub_command::Command::RefreshPrinters(_)) => {
            emit_refresh_printers_events(config, gateway, sender, &command.command_id).await
        }
        Some(hub_command::Command::RefreshPrinterMaterials(refresh)) => {
            emit_refresh_printer_materials_events(
                config,
                gateway,
                sender,
                &command.command_id,
                refresh,
            )
            .await
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
        Some(hub_command::Command::LinkPrinter(link)) => {
            emit_link_printer_events(config, gateway, sender, &command.command_id, link).await
        }
        Some(hub_command::Command::ReloadPrinterConnection(reload)) => {
            emit_reload_printer_connection_events(
                config,
                gateway,
                sender,
                &command.command_id,
                reload,
            )
            .await
        }
        Some(hub_command::Command::CameraStream(_)) => Ok(()),
        Some(
            hub_command::Command::RefreshFirmwareVersion(_)
            | hub_command::Command::PrepareFirmwareControl(_)
            | hub_command::Command::ExecuteFirmwareControl(_),
        ) => anyhow::bail!("firmware command was misrouted to non-firmware worker"),
        None => {
            reject_protocol_command(
                config,
                sender,
                &command.command_id,
                "hub command is missing its command payload".to_owned(),
            )
            .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
