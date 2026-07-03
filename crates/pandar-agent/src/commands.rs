use anyhow::{Context, anyhow};
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
    machine::{
        BambuMachineGateway, BambuPrinterEndpoint, MachineSnapshot, MaterialRefreshResult,
        discovery::DiscoveredPrinter,
    },
    protocol::agent::v1::{
        AgentEvent, CommandAck, CommandResult, LinkPrinter, PrintProjectFile, PrinterSnapshot,
        agent_event, hub_command,
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
    let command_id = command.command_id.clone();
    match command.command {
        Some(hub_command::Command::LinkPrinter(link)) => {
            emit_link_printer_events(config, gateway, sender, &command_id, link).await
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
                crate::protocol::agent::v1::HubCommand {
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

fn failure_event_with_result(
    config: &AgentConfig,
    command_id: &str,
    error: String,
    result_json: String,
) -> AgentEvent {
    result_event(config, command_id, false, error, result_json)
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

fn printer_materials_snapshot_event(
    config: &AgentConfig,
    materials: MaterialRefreshResult,
) -> AgentEvent {
    crate::machine::mqtt::printer_materials_snapshot_event(config, materials)
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
        Ok(results) => {
            for result in results {
                sender
                    .send(printer_snapshot_event(config, result.snapshot))
                    .await
                    .context("queue printer snapshot event")?;
                if let Some(materials) = result.materials {
                    sender
                        .send(printer_materials_snapshot_event(config, materials))
                        .await
                        .context("queue printer materials snapshot event")?;
                }
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

async fn emit_refresh_printer_materials_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: crate::protocol::agent::v1::RefreshPrinterMaterials,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue refresh-printer-materials command ack")?;

    match gateway
        .refresh_printer_materials(&command.serial_number, Some(&command.printer_id))
        .await
    {
        Ok(materials) => {
            sender
                .send(printer_materials_snapshot_event(config, materials))
                .await
                .context("queue printer materials snapshot event")?;
            sender
                .send(success_event(config, command_id))
                .await
                .context("queue refresh-printer-materials command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue refresh-printer-materials command failure")?;
        }
    }

    Ok(())
}

async fn emit_link_printer_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: LinkPrinter,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue link-printer command ack")?;

    let access_code = command.access_code;
    let access_code_for_error = access_code.clone();
    let printer_type = command.printer_type.trim().to_owned();
    if printer_type != "BambuLab" {
        sender
            .send(failure_event(
                config,
                command_id,
                format!("unsupported printer type {printer_type}"),
            ))
            .await
            .context("queue link-printer unsupported type failure")?;
        return Ok(());
    }

    let host = command.host.trim().to_owned();
    let endpoint =
        match discover_link_printer_endpoint(gateway, &host, &access_code, &command.name).await {
            Ok(endpoint) => endpoint,
            Err(err) => {
                let error = redact_link_error(gateway, &format!("{err:#}"), &access_code_for_error);
                sender
                    .send(failure_event(config, command_id, error))
                    .await
                    .context("queue link-printer discovery failure")?;
                return Ok(());
            }
        };

    match gateway.link_printer(endpoint.clone(), config, sender).await {
        Ok(snapshot) => {
            sender
                .send(printer_snapshot_event(config, snapshot.clone()))
                .await
                .context("queue linked printer snapshot event")?;
            let result_json = serde_json::json!({
                "type": "printer_link",
                "serial_number": snapshot.serial,
                "host": endpoint.host,
                "name": snapshot.name,
                "model": snapshot.model,
                "status": snapshot.state,
            })
            .to_string();
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue link-printer command success")?;
        }
        Err(err) => {
            let error = redact_link_error(gateway, &format!("{err:#}"), &endpoint.access_code);
            tracing::warn!(
                serial = %endpoint.serial,
                error = %error,
                "runtime printer link failed"
            );
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue link-printer command failure")?;
        }
    }

    Ok(())
}

async fn discover_link_printer_endpoint<G>(
    gateway: &G,
    host: &str,
    access_code: &str,
    name: &str,
) -> anyhow::Result<BambuPrinterEndpoint>
where
    G: BambuMachineGateway,
{
    let discovery = gateway
        .discover_printers(3)
        .await
        .with_context(|| format!("discover Bambu printer at {host}"))?;
    if let Some(printer) = discovery
        .printers
        .into_iter()
        .find(|printer| printer.host.trim() == host)
    {
        return endpoint_from_discovered_printer(host, access_code, name, printer);
    }

    let Some(printer) = gateway
        .discover_printer_at_host(host, 3)
        .await
        .with_context(|| format!("discover Bambu printer directly at {host}"))?
    else {
        return Err(anyhow!("could not discover printer at {host}"));
    };

    endpoint_from_discovered_printer(host, access_code, name, printer)
}

fn endpoint_from_discovered_printer(
    host: &str,
    access_code: &str,
    name: &str,
    printer: DiscoveredPrinter,
) -> anyhow::Result<BambuPrinterEndpoint> {
    let serial = non_blank_string(printer.serial_number.unwrap_or_default())
        .ok_or_else(|| anyhow!("printer serial could not be discovered for {host}"))?;
    Ok(BambuPrinterEndpoint {
        host: host.to_owned(),
        serial,
        access_code: access_code.to_owned(),
        name: non_blank_string(name.to_owned()),
        model: printer.model.and_then(non_blank_string),
    })
}

fn non_blank_string(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn redact_link_error<G>(gateway: &G, message: &str, access_code: &str) -> String
where
    G: BambuMachineGateway,
{
    let redacted = gateway.redact_error(message);
    if access_code.is_empty() {
        redacted
    } else {
        redacted.replace(access_code, "[REDACTED_ACCESS_CODE]")
    }
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
