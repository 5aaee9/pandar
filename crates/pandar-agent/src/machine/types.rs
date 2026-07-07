use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BambuPrinterEndpoint {
    pub host: String,
    pub serial: String,
    pub access_code: String,
    pub model: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSnapshot {
    pub serial: String,
    pub host: Option<String>,
    pub access_code: Option<String>,
    pub name: String,
    pub model: Option<String>,
    pub state: String,
    pub nozzle_temperatures: Vec<MachineNozzleTemperature>,
    pub active_nozzle: Option<String>,
    pub bed_temperature_celsius: Option<String>,
    pub bed_target_temperature_celsius: Option<String>,
    pub chamber_temperature_celsius: Option<String>,
    pub chamber_light_on: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineNozzleTemperature {
    pub label: Option<String>,
    pub current_celsius: Option<String>,
    pub target_celsius: Option<String>,
    pub diameter_mm: Option<String>,
    pub nozzle_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialRefreshResult {
    pub serial: String,
    pub printer_id: Option<String>,
    pub printer_materials_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterRefreshResult {
    pub snapshot: MachineSnapshot,
    pub materials: Option<MaterialRefreshResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrinterOperationDispatchResult {
    pub sequence_id: Option<String>,
    pub mqtt_report: Option<MachineJsonPayload>,
    pub error: Option<String>,
}

impl PrinterOperationDispatchResult {
    pub fn dispatched() -> Self {
        Self {
            sequence_id: None,
            mqtt_report: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrintProjectDispatchResult {
    pub topic: String,
    pub payload: MachineJsonPayload,
    pub qos: u8,
    pub uploaded_path: String,
    pub uploaded_url: String,
    pub md5: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MachineJsonPayload {
    Object(BTreeMap<String, MachineJsonPayload>),
    Array(Vec<MachineJsonPayload>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

impl From<Value> for MachineJsonPayload {
    fn from(value: Value) -> Self {
        serde_json::from_value(value).expect("serde_json::Value is representable")
    }
}
