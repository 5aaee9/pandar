use std::path::{Component, Path, PathBuf};

use anyhow::{Context, bail};
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{BambuMachineGateway, BambuPrinterEndpoint, MachineSnapshot},
    protocol::agent::v1::{
        AgentEvent, CommandAck, CommandResult, DiagnosePrinter, DiscoverPrinters, PrintProjectFile,
        PrinterSnapshot, agent_event, hub_command,
    },
};

pub fn parse_printer_config(raw: &str) -> anyhow::Result<Vec<BambuPrinterEndpoint>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Ok(Vec::new());
    }

    let printers: Vec<BambuPrinterEndpoint> =
        serde_json::from_str(trimmed).context("parse PANDAR_PRINTERS as JSON array")?;

    for printer in &printers {
        validate_required("host", &printer.host)?;
        validate_required("serial", &printer.serial)?;
        validate_required("access_code", &printer.access_code)?;
    }

    Ok(printers)
}

fn validate_required(field: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("PANDAR_PRINTERS printer entry has missing or blank {field}");
    }

    Ok(())
}

pub async fn handle_command_with_gateway<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command: crate::protocol::agent::v1::HubCommand,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    let artifact_reader = FilesystemArtifactReader::new(config.artifact_root.clone());
    handle_command_with_reader(config, gateway, &artifact_reader, sender, command).await
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
            emit_print_project_file_events(
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
        None => Ok(()),
    }
}

#[async_trait]
pub trait ArtifactReader: Send + Sync {
    async fn read_artifact(&self, storage_path: &str) -> anyhow::Result<Vec<u8>>;
}

pub struct FilesystemArtifactReader {
    root: PathBuf,
}

impl FilesystemArtifactReader {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl ArtifactReader for FilesystemArtifactReader {
    async fn read_artifact(&self, storage_path: &str) -> anyhow::Result<Vec<u8>> {
        let artifact_path = resolve_artifact_path(&self.root, storage_path)?;
        tokio::task::spawn_blocking(move || std::fs::read(&artifact_path))
            .await
            .context("join print artifact read task")?
            .with_context(|| format!("read print artifact {storage_path}"))
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

async fn emit_print_project_file_events<G, R>(
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
            .read_artifact(&command.storage_path)
            .await
            .with_context(|| format!("read print artifact {}", command.storage_path))?;
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

fn resolve_artifact_path(root: &Path, storage_path: &str) -> anyhow::Result<PathBuf> {
    let storage_path = Path::new(storage_path);
    if storage_path.is_absolute() {
        bail!("artifact storage path must be relative");
    }
    if storage_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("artifact storage path must not contain parent or prefix components");
    }

    Ok(root.join(storage_path))
}

fn event(config: &AgentConfig, event_id: &str, event: agent_event::Event) -> AgentEvent {
    AgentEvent {
        agent_id: config.agent_id.to_string(),
        tenant_id: config.tenant_id.to_string(),
        event_id: event_id.to_owned(),
        event: Some(event),
    }
}

#[cfg(test)]
mod tests;
