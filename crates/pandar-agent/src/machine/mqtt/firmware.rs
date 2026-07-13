mod command;
mod schema;
mod session;

use anyhow::{Context, anyhow};
use pandar_core::FirmwareAcknowledgement;
use serde::Deserialize;
use serde_json::Value;

use crate::machine::FirmwareVersionObservation;

pub(crate) use command::{FirmwareMqttCommand, FirmwareResponseDomain, firmware_command_payload};
use schema::{FirmwareAcknowledgementEnvelope, GetVersionCommandEnvelope, GetVersionReport};
pub(crate) use session::{
    FirmwareMqttSession, FirmwareMqttTaskSet, FirmwarePumpAbortHandle, firmware_mqtt_failure,
    firmware_mqtt_failure_phase,
};
#[cfg(test)]
pub(crate) use session::{
    firmware_barrier_pause, firmware_mqtt_options, firmware_pump_drop_pause,
    is_firmware_post_publish_failure, is_firmware_pre_publish_failure,
};

pub(crate) fn parse_firmware_acknowledgement(
    report: &Value,
    expected_command: &str,
    expected_sequence_id: &str,
) -> anyhow::Result<Option<FirmwareAcknowledgement>> {
    let envelope = serde_json::from_value::<FirmwareAcknowledgementEnvelope>(report.clone())
        .context("parse firmware acknowledgement envelope")?;
    let Some(fields) = envelope.upgrade else {
        return Ok(None);
    };
    let (Some(command), Some(sequence_id)) = (fields.command, fields.sequence_id) else {
        return Ok(None);
    };
    if command != expected_command || sequence_id != expected_sequence_id {
        return Ok(None);
    }
    Ok(Some(FirmwareAcknowledgement {
        command,
        sequence_id,
        result: fields.result,
        error_code: fields.error_code,
        reason: fields.reason,
        message: fields.message,
    }))
}

pub(crate) fn parse_firmware_version_observation(
    report: &Value,
) -> anyhow::Result<Option<FirmwareVersionObservation>> {
    let Some(modules) = parse_firmware_refresh_modules(report)? else {
        return Ok(None);
    };
    let model = modules
        .iter()
        .find(|module| module.name == "ota")
        .and_then(|module| module.product_name.as_deref())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("firmware get_version report missing ota product_name"))?;
    Ok(Some(FirmwareVersionObservation { model, modules }))
}

pub(crate) fn parse_firmware_refresh_modules(
    report: &Value,
) -> anyhow::Result<Option<Vec<pandar_core::PrinterFirmwareModule>>> {
    let command = serde_json::from_value::<GetVersionCommandEnvelope>(report.clone())
        .context("parse firmware get_version command envelope")?;
    if command.info.and_then(|info| info.command).as_deref() != Some("get_version") {
        return Ok(None);
    }
    let report =
        GetVersionReport::deserialize(report).context("parse firmware get_version report")?;
    let modules = report
        .info
        .module
        .ok_or_else(|| anyhow!("firmware get_version report missing info.module array"))?;
    if modules.iter().any(|module| module.name.is_empty()) {
        return Err(anyhow!(
            "firmware get_version module must have a non-empty name"
        ));
    }
    Ok(Some(modules))
}

pub(crate) fn has_non_firmware_print_telemetry(report: &Value) -> bool {
    report
        .get("print")
        .and_then(Value::as_object)
        .is_some_and(|print| {
            print
                .keys()
                .any(|key| !matches!(key.as_str(), "command" | "msg" | "cfg" | "upgrade_state"))
        })
}
