use anyhow::{Context, anyhow};
use pandar_core::{
    FirmwareAcknowledgement, PrinterFirmwareModule, PrinterFirmwareStatus, PrinterUpgradeState,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::mpsc;

use super::MachineReport;
use crate::machine::{
    FirmwareObservationCache, FirmwareStatusObservation, FirmwareVersionObservation,
};
use crate::{AgentConfig, protocol::agent::v1::AgentEvent};

use super::super::firmware::FirmwareResponseDomain;

#[derive(Deserialize)]
pub(super) struct GetVersionCommandEnvelope {
    #[serde(default)]
    pub(super) info: Option<GetVersionCommand>,
}

#[derive(Deserialize)]
pub(super) struct GetVersionCommand {
    #[serde(default)]
    pub(super) command: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct GetVersionReport {
    pub(super) info: GetVersionInfo,
}

#[derive(Deserialize)]
pub(super) struct GetVersionInfo {
    #[serde(default)]
    pub(super) module: Option<Vec<PrinterFirmwareModule>>,
}

#[derive(Deserialize)]
pub(super) struct FirmwareAcknowledgementEnvelope {
    #[serde(default)]
    pub(super) upgrade: Option<FirmwareAcknowledgementFields>,
}

#[derive(Deserialize)]
pub(super) struct FirmwareAcknowledgementFields {
    #[serde(default)]
    pub(super) command: Option<String>,
    #[serde(default)]
    pub(super) sequence_id: Option<String>,
    #[serde(default)]
    pub(super) result: Option<String>,
    #[serde(default, rename = "err_code")]
    pub(super) error_code: Option<i64>,
    #[serde(default)]
    pub(super) reason: Option<String>,
    #[serde(default)]
    pub(super) message: Option<String>,
}

#[derive(Deserialize)]
struct ReportIdentityEnvelope {
    #[serde(default)]
    info: Option<ReportIdentity>,
    #[serde(default)]
    upgrade: Option<ReportIdentity>,
}

#[derive(Deserialize)]
struct ReportIdentity {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    sequence_id: Option<String>,
}

#[derive(Deserialize)]
struct TransientStatusEnvelope {
    #[serde(default)]
    print: Option<TransientStatusFields>,
}

#[derive(Deserialize)]
struct TransientStatusFields {
    #[serde(default)]
    upgrade_state: Option<PrinterUpgradeState>,
    #[serde(default)]
    cfg: Option<String>,
}

impl MachineReport {
    pub(crate) fn firmware_acknowledgement(
        &self,
        expected_command: &str,
        expected_sequence_id: &str,
    ) -> anyhow::Result<Option<FirmwareAcknowledgement>> {
        let envelope = serde_json::from_value::<FirmwareAcknowledgementEnvelope>(self.raw.clone())
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

    pub(crate) fn firmware_version_observation(
        &self,
    ) -> anyhow::Result<Option<FirmwareVersionObservation>> {
        let Some(modules) = self.firmware_refresh_modules()? else {
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

    pub(crate) fn firmware_refresh_modules(
        &self,
    ) -> anyhow::Result<Option<Vec<PrinterFirmwareModule>>> {
        let command = serde_json::from_value::<GetVersionCommandEnvelope>(self.raw.clone())
            .context("parse firmware get_version command envelope")?;
        if command.info.and_then(|info| info.command).as_deref() != Some("get_version") {
            return Ok(None);
        }
        let report = GetVersionReport::deserialize(&self.raw)
            .context("parse firmware get_version report")?;
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

    pub(crate) fn transient_firmware_status(
        &self,
    ) -> anyhow::Result<Option<PrinterFirmwareStatus>> {
        let envelope = serde_json::from_value::<TransientStatusEnvelope>(self.raw.clone())
            .context("parse transient firmware command status")?;
        Ok(envelope.print.and_then(|print| {
            (print.upgrade_state.is_some() || print.cfg.is_some()).then_some(
                PrinterFirmwareStatus {
                    upgrade_state: print.upgrade_state,
                    cfg: print.cfg,
                },
            )
        }))
    }

    pub(crate) fn firmware_report_matches(
        &self,
        response_domain: FirmwareResponseDomain,
        command: &str,
        sequence_id: &str,
    ) -> anyhow::Result<bool> {
        let envelope = serde_json::from_value::<ReportIdentityEnvelope>(self.raw.clone())
            .context("parse firmware MQTT response identity")?;
        let identity = match response_domain {
            FirmwareResponseDomain::Info => envelope.info,
            FirmwareResponseDomain::Upgrade => envelope.upgrade,
        };
        Ok(identity.is_some_and(|identity| {
            identity.command.as_deref() == Some(command)
                && identity.sequence_id.as_deref() == Some(sequence_id)
        }))
    }

    pub(crate) fn has_non_firmware_print_telemetry(&self) -> bool {
        self.raw
            .get("print")
            .and_then(Value::as_object)
            .is_some_and(|print| {
                print
                    .keys()
                    .any(|key| !matches!(key.as_str(), "command" | "msg" | "cfg" | "upgrade_state"))
            })
    }
}

#[derive(Debug)]
pub struct FirmwareReportReducer {
    serial: String,
    generation: u64,
    revision: u64,
    reconstructed_print: Option<Value>,
    status: Option<PrinterFirmwareStatus>,
}

#[derive(Deserialize)]
struct ReportKind {
    #[serde(default)]
    msg: Option<u8>,
}

#[derive(Deserialize)]
struct FirmwarePrintFields {
    #[serde(default)]
    upgrade_state: Option<PrinterUpgradeState>,
    #[serde(default)]
    cfg: Option<String>,
}

impl FirmwareReportReducer {
    pub fn new(serial: impl Into<String>, generation: u64) -> Self {
        Self {
            serial: serial.into(),
            generation,
            revision: 0,
            reconstructed_print: None,
            status: None,
        }
    }

    pub fn observe(
        &mut self,
        report: &MachineReport,
    ) -> anyhow::Result<Option<FirmwareStatusObservation>> {
        let Some(print) = report.raw.get("print") else {
            return Ok(None);
        };
        let print = print.as_object().ok_or_else(|| {
            anyhow!(
                "parse printer {} firmware report: print must be an object",
                self.serial
            )
        })?;
        let kind = serde_json::from_value::<ReportKind>(Value::Object(print.clone()))
            .with_context(|| format!("parse printer {} firmware report msg", self.serial))?;
        let candidate = if kind.msg == Some(1) {
            let mut candidate = self
                .reconstructed_print
                .clone()
                .unwrap_or_else(|| Value::Object(Map::new()));
            deep_merge(&mut candidate, &Value::Object(print.clone()));
            candidate
        } else {
            Value::Object(print.clone())
        };
        let fields = serde_json::from_value::<FirmwarePrintFields>(candidate.clone())
            .with_context(|| {
                format!(
                    "parse printer {} firmware upgrade_state and cfg",
                    self.serial
                )
            })?;
        let status = PrinterFirmwareStatus {
            upgrade_state: fields.upgrade_state,
            cfg: fields.cfg,
        };
        self.reconstructed_print = Some(candidate);
        if self.status.is_none() && status.upgrade_state.is_none() && status.cfg.is_none() {
            return Ok(None);
        }
        if self.status.as_ref() == Some(&status) {
            return Ok(None);
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow!("firmware status revision overflow for {}", self.serial))?;
        self.status = Some(status.clone());
        Ok(Some(FirmwareStatusObservation {
            serial: self.serial.clone(),
            generation: self.generation,
            revision: self.revision,
            status,
        }))
    }

    pub(crate) async fn observe_and_commit(
        &mut self,
        report: &MachineReport,
        cache: &FirmwareObservationCache,
        config: &AgentConfig,
        sender: &mpsc::Sender<AgentEvent>,
    ) -> anyhow::Result<Option<FirmwareStatusObservation>> {
        let Some(observation) = self.observe(report)? else {
            return Ok(None);
        };
        Ok(cache
            .apply_report_status(config, observation.clone(), sender)
            .await?
            .then_some(observation))
    }
}

fn deep_merge(base: &mut Value, delta: &Value) {
    match (base, delta) {
        (Value::Object(base), Value::Object(delta)) => {
            for (key, value) in delta {
                match base.get_mut(key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, delta) => *base = delta.clone(),
    }
}
