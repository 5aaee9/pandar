use anyhow::Context;
use tokio::sync::mpsc;

use super::{
    AgentConfig, AgentEvent, BambuMachineGateway, ack_event, failure_event,
    success_event_with_result,
};
use crate::protocol::agent::v1::{DiagnosePrinter, DiscoverPrinters};

pub(super) async fn emit_discover_events<G>(
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

pub(super) async fn emit_diagnose_events<G>(
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
