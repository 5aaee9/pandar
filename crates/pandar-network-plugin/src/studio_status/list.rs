use anyhow::{Context, bail};
use serde::Deserialize;

use super::input::{ActiveTray, AmsUnit, MaterialTray, PrinterHms};

#[derive(Deserialize)]
struct PrinterList {
    message: String,
    devices: Vec<Printer>,
}

#[derive(Deserialize)]
struct Printer {
    dev_id: String,
    #[serde(rename = "dev_name")]
    _dev_name: String,
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "dev_ip")]
    _dev_ip: Option<String>,
    #[serde(rename = "dev_access_code")]
    _dev_access_code: Option<String>,
    #[serde(rename = "dev_model_name")]
    _dev_model_name: Option<String>,
    #[serde(rename = "model")]
    _model: Option<String>,
    #[serde(rename = "dev_online")]
    _dev_online: bool,
    #[serde(rename = "online")]
    _online: bool,
    #[serde(rename = "task_status")]
    _task_status: String,
    #[serde(rename = "state")]
    _state: String,
    #[serde(rename = "gcode_state")]
    _gcode_state: Option<String>,
    #[serde(rename = "mc_percent")]
    _mc_percent: Option<u8>,
    #[serde(rename = "mc_remaining_time")]
    _mc_remaining_time: Option<u32>,
    #[serde(rename = "layer_num")]
    _layer_num: Option<u32>,
    #[serde(rename = "total_layer_num")]
    _total_layer_num: Option<u32>,
    #[serde(rename = "task_id")]
    _task_id: Option<String>,
    #[serde(rename = "subtask_id")]
    _subtask_id: Option<String>,
    #[serde(rename = "gcode_file")]
    _gcode_file: Option<String>,
    #[serde(rename = "subtask_name")]
    _subtask_name: Option<String>,
    #[serde(rename = "hms")]
    _hms: Vec<PrinterHms>,
    #[serde(rename = "pandar_printer_id")]
    _pandar_printer_id: String,
    #[serde(rename = "nozzle_temperatures")]
    _nozzle_temperatures: Vec<ValidatedNozzleTemperature>,
    #[serde(rename = "active_nozzle")]
    _active_nozzle: Option<String>,
    #[serde(rename = "bed_temperature_celsius")]
    _bed_temperature_celsius: Option<String>,
    #[serde(rename = "bed_target_temperature_celsius")]
    _bed_target_temperature_celsius: Option<String>,
    #[serde(rename = "chamber_temperature_celsius")]
    _chamber_temperature_celsius: Option<String>,
    #[serde(rename = "chamber_light_on")]
    _chamber_light_on: Option<bool>,
    #[serde(rename = "materials")]
    _materials: Option<ValidatedMaterials>,
}

#[derive(Deserialize)]
struct ValidatedNozzleTemperature {
    #[serde(rename = "label")]
    _label: Option<String>,
    #[serde(rename = "current_celsius")]
    _current_celsius: Option<String>,
    #[serde(rename = "target_celsius")]
    _target_celsius: Option<String>,
    #[serde(rename = "diameter_mm")]
    _diameter_mm: Option<String>,
    #[serde(rename = "nozzle_type")]
    _nozzle_type: Option<String>,
}

#[derive(Deserialize)]
struct ValidatedMaterials {
    #[serde(rename = "ams_units")]
    _ams_units: Vec<AmsUnit>,
    #[serde(rename = "external_spools")]
    _external_spools: Vec<MaterialTray>,
    #[serde(rename = "active_tray")]
    _active_tray: Option<ActiveTray>,
    #[serde(rename = "observed_at")]
    _observed_at: String,
}

pub(crate) fn validate_printer_list(body: &str) -> anyhow::Result<()> {
    let response = serde_json::from_str::<PrinterList>(body)
        .context("deserialize Hub plugin printer status response")?;
    if response.message != "success" {
        bail!("Hub plugin printer status response was not successful");
    }
    if response
        .devices
        .iter()
        .any(|printer| printer.dev_id.trim().is_empty())
    {
        bail!("Hub plugin printer status response contained an empty device id");
    }
    Ok(())
}
