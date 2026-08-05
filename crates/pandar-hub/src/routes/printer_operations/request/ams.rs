use super::super::invalid_printer_control;
use super::PrinterOperationRequest;
use crate::{repositories::PrinterOperationKind, routes::ApiError};

impl PrinterOperationRequest {
    pub(super) fn into_ams_operation(self) -> Result<PrinterOperationKind, ApiError> {
        match self.action.as_str() {
            "ams_reread_rfid"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some()
                    && self.global_tray_id.is_missing()
                    && self.external_id.is_missing()
                    && self.extruder_id.is_missing()
                    && self.no_drying_fields()
                    && self.no_rack_fields() =>
            {
                Ok(PrinterOperationKind::AmsRereadRfid {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                })
            }
            "ams_load_filament"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some()
                    && self.no_drying_fields()
                    && self.no_rack_fields() =>
            {
                Ok(PrinterOperationKind::AmsLoadFilament {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                    global_tray_id: self.global_tray_id.into_option(),
                    external_id: self.external_id.into_option(),
                    extruder_id: self.extruder_id.into_option(),
                })
            }
            "ams_unload_filament"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_some()
                    && self.no_drying_fields()
                    && self.no_rack_fields() =>
            {
                Ok(PrinterOperationKind::AmsUnloadFilament {
                    ams_id: self.ams_id.expect("checked above"),
                    slot_id: self.slot_id.expect("checked above"),
                    global_tray_id: self.global_tray_id.into_option(),
                    external_id: self.external_id.into_option(),
                    extruder_id: self.extruder_id.into_option(),
                })
            }
            "ams_start_drying"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_missing()
                    && self.global_tray_id.is_missing()
                    && self.external_id.is_missing()
                    && self.extruder_id.is_missing()
                    && self.light_on.is_missing()
                    && self.no_rack_fields()
                    && self.temperature_celsius.is_some()
                    && self.duration_hours.is_some()
                    && self.filament.is_some() =>
            {
                Ok(PrinterOperationKind::AmsStartDrying {
                    ams_id: self.ams_id.expect("checked above"),
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    duration_hours: self.duration_hours.expect("checked above"),
                    filament: self.filament.expect("checked above"),
                    rotate_tray: self.rotate_tray.unwrap_or(false),
                })
            }
            "ams_stop_drying"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_some()
                    && self.slot_id.is_missing()
                    && self.global_tray_id.is_missing()
                    && self.external_id.is_missing()
                    && self.extruder_id.is_missing()
                    && self.light_on.is_missing()
                    && self.no_rack_fields()
                    && self.no_drying_fields() =>
            {
                Ok(PrinterOperationKind::AmsStopDrying {
                    ams_id: self.ams_id.expect("checked above"),
                })
            }
            _ => Err(invalid_printer_control()),
        }
    }
}
