use std::collections::HashSet;

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::{AgentConfig, commands::parse_printer_config, machine::BambuPrinterEndpoint};

pub(crate) async fn startup_printers(
    config: &AgentConfig,
) -> anyhow::Result<Vec<BambuPrinterEndpoint>> {
    let mut printers = parse_printer_config(&config.printers)?;
    let Some(hub_api_url) = config
        .hub_api_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(printers);
    };

    let saved = fetch_saved_printer_connections(config, hub_api_url)
        .await
        .context("load saved printer connections from pandar-hub")?;
    let mut configured_serials = printers
        .iter()
        .map(|printer| printer.serial.clone())
        .collect::<HashSet<_>>();
    for printer in saved {
        if configured_serials.insert(printer.serial.clone()) {
            printers.push(printer);
        }
    }

    Ok(printers)
}

async fn fetch_saved_printer_connections(
    config: &AgentConfig,
    hub_api_url: &str,
) -> anyhow::Result<Vec<BambuPrinterEndpoint>> {
    let url = format!(
        "{}/api/v1/agents/{}/printers",
        hub_api_url.trim_end_matches('/'),
        config.agent_id
    );
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(&config.agent_credential)
        .send()
        .await
        .context("request saved printer connections")?;
    let status = response.status();
    if !status.is_success() {
        bail!("hub saved printer connection request failed with HTTP {status}");
    }

    let response = response
        .json::<SavedPrinterConnectionsResponse>()
        .await
        .context("decode saved printer connections")?;
    response
        .printers
        .into_iter()
        .map(saved_printer_endpoint)
        .collect()
}

#[derive(Debug, Deserialize)]
struct SavedPrinterConnectionsResponse {
    printers: Vec<SavedPrinterConnection>,
}

#[derive(Debug, Deserialize)]
struct SavedPrinterConnection {
    serial: String,
    host: String,
    access_code: String,
    name: String,
    model: Option<String>,
}

fn saved_printer_endpoint(value: SavedPrinterConnection) -> anyhow::Result<BambuPrinterEndpoint> {
    validate_required("serial", &value.serial)?;
    validate_required("host", &value.host)?;
    validate_required("access_code", &value.access_code)?;
    Ok(BambuPrinterEndpoint {
        host: value.host,
        serial: value.serial,
        access_code: value.access_code,
        model: value.model,
        name: Some(value.name),
    })
}

fn validate_required(field: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("hub saved printer connection has missing or blank {field}");
    }

    Ok(())
}
