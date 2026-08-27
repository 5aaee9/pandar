use std::{collections::BTreeSet, env, time::Duration};

use anyhow::{Context, ensure};
use pandar_agent::machine::{
    BambuPrinterEndpoint,
    compatibility::normalize_model,
    file_transfer::MachineFileTransfer,
    ftps::FtpsMachineFileTransfer,
    mqtt::{RumqttcBambuMqttTransport, read_firmware_version},
};
use pandar_core::PrinterFirmwareModule;
use serde::{Deserialize, Serialize};

const TARGETS_ENV: &str = "PANDAR_A1_FTPS_VALIDATION_TARGETS";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationTarget {
    host: String,
    serial: String,
    access_code: String,
    model: String,
}

#[derive(Serialize)]
struct FirmwareModuleEvidence {
    name: String,
    software_version: Option<String>,
    hardware_version: Option<String>,
}

#[derive(Serialize)]
struct ValidationEvidence {
    model: String,
    firmware_modules: Vec<FirmwareModuleEvidence>,
    ftps_data_protection: &'static str,
    root_entry_count: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let targets_json = env::var(TARGETS_ENV)
        .with_context(|| format!("{TARGETS_ENV} must contain the two validation targets"))?;
    let targets: Vec<ValidationTarget> = serde_json::from_str(&targets_json)
        .with_context(|| format!("parse {TARGETS_ENV} as JSON"))?;
    validate_target_set(&targets)?;

    let mut evidence = Vec::with_capacity(targets.len());
    for target in targets {
        evidence.push(validate_target(target).await?);
    }
    evidence.sort_by(|left, right| left.model.cmp(&right.model));

    println!(
        "{}",
        serde_json::to_string_pretty(&evidence).context("encode validation evidence")?
    );
    Ok(())
}

fn validate_target_set(targets: &[ValidationTarget]) -> anyhow::Result<()> {
    let models = targets
        .iter()
        .map(|target| normalize_model(&target.model))
        .collect::<Option<BTreeSet<_>>>()
        .context("validation target model must not be blank")?;
    let expected = BTreeSet::from(["A1".to_owned(), "A1_MINI".to_owned()]);

    ensure!(
        targets.len() == 2 && models == expected,
        "validation requires exactly one A1 and one A1 Mini target"
    );
    Ok(())
}

async fn validate_target(target: ValidationTarget) -> anyhow::Result<ValidationEvidence> {
    let expected_model = normalize_model(&target.model).expect("target set was validated");
    let endpoint = BambuPrinterEndpoint {
        host: target.host,
        serial: target.serial,
        access_code: target.access_code,
        model: Some(target.model),
        name: None,
    };

    let mqtt = RumqttcBambuMqttTransport::connect(&endpoint);
    let firmware = read_firmware_version(&mqtt, &endpoint, Duration::from_secs(15))
        .await
        .with_context(|| format!("read {expected_model} firmware version"))?;
    let actual_model =
        normalize_model(&firmware.model).context("firmware model must not be blank")?;
    ensure!(
        actual_model == expected_model,
        "configured model {expected_model} does not match firmware model {actual_model}"
    );

    let entries = FtpsMachineFileTransfer::new(endpoint)
        .list("/")
        .await
        .with_context(|| format!("list {actual_model} FTPS root with PROT P"))?;

    Ok(ValidationEvidence {
        model: actual_model,
        firmware_modules: firmware
            .modules
            .into_iter()
            .map(FirmwareModuleEvidence::from)
            .collect(),
        ftps_data_protection: "PROT P",
        root_entry_count: entries.len(),
    })
}

impl From<PrinterFirmwareModule> for FirmwareModuleEvidence {
    fn from(module: PrinterFirmwareModule) -> Self {
        Self {
            name: module.name,
            software_version: module.software_version,
            hardware_version: module.hardware_version,
        }
    }
}
