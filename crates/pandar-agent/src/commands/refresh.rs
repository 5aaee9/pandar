use anyhow::Context;
use tokio::sync::mpsc;

use crate::{AgentConfig, machine::BambuMachineGateway};
use pandar_protocol::agent::v1::AgentEvent;

use super::responses::{
    ack_event, failure_event, printer_materials_snapshot_event, printer_snapshot_event,
    success_event,
};

pub(super) async fn emit_refresh_printers_events<G>(
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

pub(super) async fn emit_refresh_printer_materials_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: pandar_protocol::agent::v1::RefreshPrinterMaterials,
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
