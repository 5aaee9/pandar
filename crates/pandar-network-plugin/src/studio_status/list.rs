use anyhow::{Context, bail};
use pandar_core::PrinterFirmwareState;
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
    #[serde(rename = "print_error")]
    _print_error: Option<u32>,
    #[serde(rename = "job_id")]
    _job_id: Option<String>,
    #[serde(rename = "subtask_id")]
    _subtask_id: Option<String>,
    #[serde(rename = "gcode_file")]
    _gcode_file: Option<String>,
    #[serde(rename = "subtask_name")]
    _subtask_name: Option<String>,
    #[serde(rename = "hms")]
    _hms: Vec<PrinterHms>,
    pandar_printer_id: String,
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
    firmware: Option<PrinterFirmwareState>,
}

pub(crate) struct FirmwareObservation {
    pub(crate) dev_id: String,
    pub(crate) pandar_printer_id: String,
    pub(crate) firmware: Option<PrinterFirmwareState>,
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
    let response = parse_printer_list(body)?;
    validate_response(&response)
}

pub(crate) fn firmware_observations(body: &str) -> anyhow::Result<Vec<FirmwareObservation>> {
    let response = parse_printer_list(body)?;
    validate_response(&response)?;
    Ok(response
        .devices
        .into_iter()
        .map(|printer| FirmwareObservation {
            dev_id: printer.dev_id,
            pandar_printer_id: printer.pandar_printer_id,
            firmware: printer.firmware,
        })
        .collect())
}

fn parse_printer_list(body: &str) -> anyhow::Result<PrinterList> {
    serde_json::from_str::<PrinterList>(body)
        .context("deserialize Hub plugin printer status response")
}

fn validate_response(response: &PrinterList) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::validate_printer_list;

    fn printer_list_with_fields(fields: serde_json::Value) -> String {
        let mut printer = serde_json::json!({
            "dev_id": "studio-serial-1",
            "dev_name": "Probe Printer",
            "name": "Probe Printer",
            "dev_ip": "192.0.2.10",
            "dev_access_code": "12345678",
            "dev_model_name": "N6",
            "model": "N6",
            "dev_online": true,
            "online": true,
            "task_status": "RUNNING",
            "state": "RUNNING",
            "gcode_state": "RUNNING",
            "mc_percent": 37,
            "mc_remaining_time": 52,
            "layer_num": 12,
            "total_layer_num": 120,
            "task_id": "task-42",
            "subtask_id": "subtask-42",
            "gcode_file": "drawer-organizer.gcode",
            "subtask_name": "drawer-organizer",
            "hms": [],
            "pandar_printer_id": "printer-1",
            "nozzle_temperatures": [],
            "active_nozzle": null,
            "bed_temperature_celsius": null,
            "bed_target_temperature_celsius": null,
            "chamber_temperature_celsius": null,
            "chamber_light_on": null,
            "materials": null
        });
        printer
            .as_object_mut()
            .expect("printer fixture is an object")
            .extend(
                fields
                    .as_object()
                    .expect("fields fixture is an object")
                    .clone(),
            );
        serde_json::json!({"message": "success", "devices": [printer]}).to_string()
    }

    #[test]
    fn studio_status_list_rejects_wrong_print_error_type() {
        let body = printer_list_with_fields(serde_json::json!({"print_error": "83918929"}));

        assert!(validate_printer_list(&body).is_err());
    }

    #[test]
    fn studio_status_list_rejects_wrong_job_id_type() {
        let body = printer_list_with_fields(serde_json::json!({"job_id": 42}));

        assert!(validate_printer_list(&body).is_err());
    }
}
