use anyhow::Context;

use crate::machine::BambuPrinterEndpoint;

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
