use anyhow::Context;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{BambuMachineGateway, BambuPrinterEndpoint, MachineSnapshot},
    protocol::agent::v1::{
        AgentEvent, CommandAck, CommandResult, PrinterSnapshot, agent_event, hub_command,
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
    match command.command {
        Some(hub_command::Command::RefreshPrinters(_)) => {
            emit_refresh_printers_events(config, gateway, sender, &command.command_id).await
        }
        None => Ok(()),
    }
}

pub fn ack_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    event(
        config,
        "ack",
        agent_event::Event::CommandAck(CommandAck {
            command_id: command_id.to_owned(),
            accepted: true,
            error: String::new(),
        }),
    )
}

pub fn success_event(config: &AgentConfig, command_id: &str) -> AgentEvent {
    result_event(config, command_id, true, String::new())
}

fn failure_event(config: &AgentConfig, command_id: &str, error: String) -> AgentEvent {
    result_event(config, command_id, false, error)
}

fn result_event(
    config: &AgentConfig,
    command_id: &str,
    success: bool,
    error: String,
) -> AgentEvent {
    event(
        config,
        if success { "success" } else { "failure" },
        agent_event::Event::CommandResult(CommandResult {
            command_id: command_id.to_owned(),
            success,
            error,
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
            sender
                .send(failure_event(config, command_id, format!("{err:#}")))
                .await
                .context("queue refresh-printers command failure")?;
        }
    }

    Ok(())
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
