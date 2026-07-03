use serde::Deserialize;

use crate::{
    repositories::{PrinterAxis, PrinterAxisMovement, PrinterOperationKind},
    routes::ApiError,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrinterOperationRequest {
    action: String,
    #[serde(default)]
    speed_mode: Option<u8>,
    #[serde(default)]
    axes: Vec<PrinterAxis>,
    #[serde(default)]
    movements: Vec<PrinterAxisMovement>,
    #[serde(default)]
    feedrate_mm_per_min: Option<u32>,
    #[serde(default)]
    temperature_celsius: Option<u16>,
    #[serde(default)]
    wait: Option<bool>,
    #[serde(default)]
    ams_id: Option<u32>,
    #[serde(default)]
    slot_id: Option<u32>,
    #[serde(default)]
    global_tray_id: Option<u32>,
    #[serde(default)]
    external_id: Option<String>,
    #[serde(default)]
    extruder_id: Option<u32>,
}

impl PrinterOperationRequest {
    pub(super) fn into_operation(self) -> Result<PrinterOperationKind, ApiError> {
        match self.action.as_str() {
            "pause" if self.no_operation_fields() => Ok(PrinterOperationKind::Pause),
            "resume" if self.no_operation_fields() => Ok(PrinterOperationKind::Resume),
            "stop" if self.no_operation_fields() => Ok(PrinterOperationKind::Stop),
            "set_print_speed"
                if self.speed_mode.is_some()
                    && self.axes.is_empty()
                    && self.movements.is_empty()
                    && self.feedrate_mm_per_min.is_none()
                    && self.temperature_celsius.is_none()
                    && self.wait.is_none()
                    && self.no_ams_fields() =>
            {
                Ok(PrinterOperationKind::SetPrintSpeed {
                    speed_mode: self.speed_mode.expect("checked above"),
                })
            }
            "home"
                if self.speed_mode.is_none()
                    && self.movements.is_empty()
                    && self.feedrate_mm_per_min.is_none()
                    && self.temperature_celsius.is_none()
                    && self.wait.is_none()
                    && self.no_ams_fields() =>
            {
                Ok(PrinterOperationKind::Home { axes: self.axes })
            }
            "move_axes"
                if self.speed_mode.is_none()
                    && self.axes.is_empty()
                    && self.temperature_celsius.is_none()
                    && self.wait.is_none()
                    && self.no_ams_fields() =>
            {
                Ok(PrinterOperationKind::MoveAxes {
                    movements: self.movements,
                    feedrate_mm_per_min: self.feedrate_mm_per_min,
                })
            }
            "set_hotend_temperature"
                if self.speed_mode.is_none()
                    && self.axes.is_empty()
                    && self.movements.is_empty()
                    && self.feedrate_mm_per_min.is_none()
                    && self.no_ams_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetHotendTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                })
            }
            "ams_reread_rfid"
                if self.speed_mode.is_none()
                    && self.axes.is_empty()
                    && self.movements.is_empty()
                    && self.feedrate_mm_per_min.is_none()
                    && self.temperature_celsius.is_none()
                    && self.wait.is_none()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some()
                    && self.global_tray_id.is_none()
                    && self.external_id.is_none()
                    && self.extruder_id.is_none() =>
            {
                Ok(PrinterOperationKind::AmsRereadRfid {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                })
            }
            "ams_load_filament"
                if self.speed_mode.is_none()
                    && self.axes.is_empty()
                    && self.movements.is_empty()
                    && self.feedrate_mm_per_min.is_none()
                    && self.temperature_celsius.is_none()
                    && self.wait.is_none()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some() =>
            {
                Ok(PrinterOperationKind::AmsLoadFilament {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                    global_tray_id: self.global_tray_id,
                    external_id: self.external_id,
                    extruder_id: self.extruder_id,
                })
            }
            "ams_unload_filament"
                if self.speed_mode.is_none()
                    && self.axes.is_empty()
                    && self.movements.is_empty()
                    && self.feedrate_mm_per_min.is_none()
                    && self.temperature_celsius.is_none()
                    && self.wait.is_none()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some() =>
            {
                Ok(PrinterOperationKind::AmsUnloadFilament {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                    global_tray_id: self.global_tray_id,
                    external_id: self.external_id,
                    extruder_id: self.extruder_id,
                })
            }
            _ => Err(ApiError::bad_request("invalid_printer_control")),
        }
    }

    fn no_operation_fields(&self) -> bool {
        self.speed_mode.is_none()
            && self.axes.is_empty()
            && self.movements.is_empty()
            && self.feedrate_mm_per_min.is_none()
            && self.temperature_celsius.is_none()
            && self.wait.is_none()
            && self.no_ams_fields()
    }

    fn no_ams_fields(&self) -> bool {
        self.ams_id.is_none()
            && self.slot_id.is_none()
            && self.global_tray_id.is_none()
            && self.external_id.is_none()
            && self.extruder_id.is_none()
    }
}
