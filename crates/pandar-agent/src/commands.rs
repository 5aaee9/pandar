use anyhow::Context;
use tokio::sync::mpsc;

mod artifacts;
mod config;
mod events;
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
    machine::{BambuMachineGateway, MachineSnapshot, PrinterControl as MachinePrinterControl},
    protocol::agent::v1::{
        AgentEvent, CommandAck, CommandResult, DiagnosePrinter, DiscoverPrinters, PrintProjectFile,
        PrinterControl as ProtoPrinterControl, PrinterSnapshot, agent_event, hub_command,
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
            emit_discover_printers_events(config, gateway, sender, &command.command_id, discovery)
                .await
        }
        Some(hub_command::Command::DiagnosePrinter(diagnostic)) => {
            emit_diagnose_printer_events(config, gateway, sender, &command.command_id, diagnostic)
                .await
        }
        Some(hub_command::Command::PrinterControl(control)) => {
            emit_printer_control_events(config, gateway, sender, &command.command_id, control).await
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

async fn emit_printer_control_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: ProtoPrinterControl,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    let control = match parse_printer_control(&command) {
        Ok(control) => control,
        Err(err) => {
            sender
                .send(rejected_ack_event(config, command_id, format!("{err:#}")))
                .await
                .context("queue printer-control rejected ack")?;
            return Ok(());
        }
    };

    if let Err(err) = gateway.validate_printer(&command.serial_number).await {
        let error = gateway.redact_error(&format!("{err:#}"));
        sender
            .send(rejected_ack_event(config, command_id, error))
            .await
            .context("queue printer-control rejected ack")?;
        return Ok(());
    }

    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue printer-control command ack")?;

    match gateway
        .control_printer(&command.serial_number, control)
        .await
        .with_context(|| {
            format!(
                "dispatch printer control {} to {}",
                command.action, command.serial_number
            )
        }) {
        Ok(()) => {
            let result_json = printer_control_result_json(&command);
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue printer-control command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue printer-control command failure")?;
        }
    }

    Ok(())
}

fn parse_printer_control(command: &ProtoPrinterControl) -> anyhow::Result<MachinePrinterControl> {
    match command.action.as_str() {
        "pause" if command.speed_mode == 0 => Ok(MachinePrinterControl::Pause),
        "resume" if command.speed_mode == 0 => Ok(MachinePrinterControl::Resume),
        "stop" if command.speed_mode == 0 => Ok(MachinePrinterControl::Stop),
        "pause" | "resume" | "stop" => {
            anyhow::bail!("printer control speed_mode is only valid for set_print_speed")
        }
        "set_print_speed" => match command.speed_mode {
            1..=4 => Ok(MachinePrinterControl::SetPrintSpeed(
                command.speed_mode as u8,
            )),
            _ => anyhow::bail!("invalid printer control speed_mode; expected 1..=4"),
        },
        action => anyhow::bail!("unknown printer control action {action}"),
    }
}

fn printer_control_result_json(command: &ProtoPrinterControl) -> String {
    let mut result = serde_json::Map::new();
    result.insert("type".to_string(), serde_json::json!("printer_control"));
    result.insert("action".to_string(), serde_json::json!(command.action));
    result.insert(
        "serial_number".to_string(),
        serde_json::json!(command.serial_number),
    );
    if command.action == "set_print_speed" {
        result.insert(
            "speed_mode".to_string(),
            serde_json::json!(command.speed_mode),
        );
    }
    serde_json::Value::Object(result).to_string()
}

async fn emit_discover_printers_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: DiscoverPrinters,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue discover-printers command ack")?;

    let result = async {
        let discovery = gateway
            .discover_printers(command.timeout_seconds)
            .await
            .context("run printer discovery")?;
        serde_json::to_string(&discovery).context("serialize printer discovery result")
    }
    .await;

    match result {
        Ok(result_json) => {
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue discover-printers command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue discover-printers command failure")?;
        }
    }

    Ok(())
}

async fn emit_diagnose_printer_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: DiagnosePrinter,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue diagnose-printer command ack")?;

    let result = async {
        let diagnostic = gateway
            .diagnose_printer(&command.serial_number)
            .await
            .context("run printer diagnostic")?;
        serde_json::to_string(&diagnostic).context("serialize printer diagnostic result")
    }
    .await;

    match result {
        Ok(result_json) => {
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue diagnose-printer command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue diagnose-printer command failure")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
