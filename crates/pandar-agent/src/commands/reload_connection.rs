use anyhow::{Context, anyhow};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{BambuMachineGateway, BambuPrinterEndpoint},
    startup::fetch_saved_printer_connections,
};
use pandar_protocol::agent::v1::{AgentEvent, ReloadPrinterConnection};

use super::responses::{ack_event, failure_event, success_event_with_result};

pub(super) async fn emit_reload_printer_connection_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: ReloadPrinterConnection,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue reload-printer-connection command ack")?;

    let result = reload_printer_connection(config, gateway, sender, &command).await;
    match result {
        Ok(endpoint) => {
            let result_json = serde_json::to_string(&PrinterConnectionReloadResult {
                kind: "printer_connection_reload",
                printer_id: &command.printer_id,
                serial_number: &endpoint.serial,
                host: &endpoint.host,
            })
            .expect("printer connection reload result is serializable");
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue reload-printer-connection command success")?;
        }
        Err((error, access_code)) => {
            let error = redact_reload_error(gateway, &format!("{error:#}"), access_code.as_deref());
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue reload-printer-connection command failure")?;
        }
    }

    Ok(())
}

async fn reload_printer_connection<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command: &ReloadPrinterConnection,
) -> Result<BambuPrinterEndpoint, (anyhow::Error, Option<String>)>
where
    G: BambuMachineGateway,
{
    let hub_api_url = config
        .hub_api_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                anyhow!("PANDAR_HUB_API_URL is required to reload printer connections"),
                None,
            )
        })?;
    let endpoint = fetch_saved_printer_connections(config, hub_api_url)
        .await
        .context("reload saved printer connections from pandar-hub")
        .map_err(|error| (error, None))?
        .into_iter()
        .find(|endpoint| endpoint.serial == command.serial_number)
        .ok_or_else(|| {
            (
                anyhow!(
                    "saved printer connection {} is no longer assigned to this agent",
                    command.serial_number
                ),
                None,
            )
        })?;
    let access_code = Some(endpoint.access_code.clone());
    gateway
        .validate_printer_endpoint_identity(&endpoint)
        .await
        .context("validate reloaded printer endpoint identity")
        .map_err(|error| (error, access_code.clone()))?;
    gateway
        .link_printer(endpoint.clone(), config, sender)
        .await
        .context("replace runtime printer connection")
        .map_err(|error| (error, access_code))?;
    Ok(endpoint)
}

fn redact_reload_error<G>(gateway: &G, message: &str, access_code: Option<&str>) -> String
where
    G: BambuMachineGateway,
{
    let redacted = gateway.redact_error(message);
    match access_code {
        Some(access_code) if !access_code.is_empty() => {
            redacted.replace(access_code, "[REDACTED_ACCESS_CODE]")
        }
        _ => redacted,
    }
}

#[derive(Serialize)]
struct PrinterConnectionReloadResult<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    printer_id: &'a str,
    serial_number: &'a str,
    host: &'a str,
}
