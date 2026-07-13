use anyhow::{Context, anyhow};
use pandar_core::PrinterFirmwareStatus;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::mpsc;

use super::{FirmwareObservationCache, types::FirmwareStatusObservation};
use crate::{AgentConfig, protocol::agent::v1::AgentEvent};

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
    upgrade_state: Option<pandar_core::PrinterUpgradeState>,
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

    pub fn observe(&mut self, report: &Value) -> anyhow::Result<Option<FirmwareStatusObservation>> {
        let Some(print) = report.get("print") else {
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
        report: &Value,
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
