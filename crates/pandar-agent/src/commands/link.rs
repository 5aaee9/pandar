use anyhow::{Context, anyhow};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::{BambuMachineGateway, BambuPrinterEndpoint, discovery::DiscoveredPrinter},
    protocol::agent::v1::{AgentEvent, LinkPrinter},
};

use super::responses::{ack_event, failure_event, success_event_with_result};

pub(super) async fn emit_link_printer_events<G>(
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
            let result_json = serde_json::to_string(&PrinterLinkResult {
                kind: "printer_link",
                serial_number: &snapshot.serial,
                host: &endpoint.host,
                name: &snapshot.name,
                model: snapshot.model.as_deref(),
                status: &snapshot.state,
            })
            .expect("printer link result is serializable");
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

#[derive(Serialize)]
struct PrinterLinkResult<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    serial_number: &'a str,
    host: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    status: &'a str,
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
