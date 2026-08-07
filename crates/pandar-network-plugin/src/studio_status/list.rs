use anyhow::{Context, bail};
use pandar_core::PrinterFirmwareState;
use serde::Deserialize;

use super::{input::PrinterStatus, payload::push_status_json};

#[derive(Deserialize)]
struct PrinterList {
    message: String,
    devices: Vec<HubPrinter>,
}

#[derive(Deserialize)]
struct HubPrinter {
    dev_id: String,
    pandar_printer_id: String,
    dev_online: bool,
    online: bool,
    #[serde(flatten)]
    status: PrinterStatus,
    firmware: Option<PrinterFirmwareState>,
}

pub struct StudioStatusProjection {
    printers: Vec<PrinterObservation>,
    firmware: FirmwareProjection,
}

#[derive(Clone)]
pub struct PrinterObservation {
    pub(crate) dev_id: String,
    pub(crate) pandar_printer_id: String,
    pub(crate) model: Option<String>,
    pub(crate) status_report: String,
    pub(crate) online: bool,
    pub(crate) studio_local_camera: bool,
}

pub struct FirmwareProjection {
    source_len: usize,
    observations: Vec<FirmwareObservation>,
}

pub(crate) struct FirmwareObservation {
    pub(crate) dev_id: String,
    pub(crate) firmware: Option<PrinterFirmwareState>,
}

impl StudioStatusProjection {
    pub fn printers(&self) -> &[PrinterObservation] {
        &self.printers
    }

    pub fn into_firmware(self) -> FirmwareProjection {
        self.firmware
    }

    pub(crate) fn into_parts(self) -> (Vec<PrinterObservation>, FirmwareProjection) {
        (self.printers, self.firmware)
    }
}

impl PrinterObservation {
    pub fn status_report(&self) -> &str {
        &self.status_report
    }
}

impl FirmwareProjection {
    pub(crate) fn source_len(&self) -> usize {
        self.source_len
    }

    pub(crate) fn observations(&self) -> &[FirmwareObservation] {
        &self.observations
    }
}

pub fn project_hub_printers(body: &str) -> anyhow::Result<StudioStatusProjection> {
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

    let mut printers = Vec::with_capacity(response.devices.len());
    let mut firmware = Vec::with_capacity(response.devices.len());
    for printer in response.devices {
        let online = printer.dev_online && printer.online;
        let model = printer.status.dev_model_name.clone();
        let studio_local_camera = printer.status.studio_local_camera;
        let status_report = push_status_json(&printer.status, online);
        firmware.push(FirmwareObservation {
            dev_id: printer.dev_id.clone(),
            firmware: printer.firmware,
        });
        printers.push(PrinterObservation {
            dev_id: printer.dev_id,
            pandar_printer_id: printer.pandar_printer_id,
            model,
            status_report,
            online,
            studio_local_camera,
        });
    }

    Ok(StudioStatusProjection {
        printers,
        firmware: FirmwareProjection {
            source_len: body.len(),
            observations: firmware,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::project_hub_printers;

    fn printer_list_with_fields(fields: serde_json::Value) -> String {
        let mut printer = serde_json::json!({
            "dev_id": "studio-serial-1",
            "dev_name": "Probe Printer",
            "name": "Probe Printer",
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
            "chamber_target_temperature_celsius": null,
            "chamber_light_on": null,
            "materials": null,
            "firmware": null
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
    fn projection_rejects_wrong_known_field_types() {
        for fields in [
            serde_json::json!({"print_error": "83918929"}),
            serde_json::json!({"job_id": 42}),
            serde_json::json!({"bed_temperature_celsius": 60}),
        ] {
            assert!(project_hub_printers(&printer_list_with_fields(fields)).is_err());
        }
    }

    #[test]
    fn projection_accepts_additive_unknown_fields() {
        assert!(
            project_hub_printers(&printer_list_with_fields(
                serde_json::json!({"future_status_field": {"enabled": true}})
            ))
            .is_ok()
        );
    }

    #[test]
    fn projection_requires_both_online_signals() {
        for fields in [
            serde_json::json!({"dev_online": false, "online": true}),
            serde_json::json!({"dev_online": true, "online": false}),
        ] {
            let projection = project_hub_printers(&printer_list_with_fields(fields)).unwrap();
            assert!(!projection.printers()[0].online);
        }
    }

    #[test]
    fn projection_keeps_missing_model_unknown() {
        let projection = project_hub_printers(&printer_list_with_fields(serde_json::json!({
            "dev_model_name": null,
            "model": null
        })))
        .unwrap();

        assert!(projection.printers()[0].model.is_none());
    }
}
