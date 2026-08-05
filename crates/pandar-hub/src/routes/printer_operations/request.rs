use serde::Deserialize;

mod ams;
mod plugin;

use super::{device_features, invalid_printer_control, request_field::RequestField};
use crate::{
    grpc::commands::RequiredDeviceFeature,
    repositories::{PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperationKind},
    routes::ApiError,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::routes) struct PrinterOperationRequest {
    pub(super) action: String,
    #[serde(default)]
    pub(super) param: RequestField<String>,
    #[serde(default)]
    speed_mode: RequestField<u8>,
    #[serde(default)]
    axes: RequestField<Vec<PrinterAxis>>,
    #[serde(default)]
    movements: RequestField<Vec<PrinterAxisMovement>>,
    #[serde(default)]
    feedrate_mm_per_min: RequestField<u32>,
    #[serde(default)]
    temperature_celsius: RequestField<u16>,
    #[serde(default)]
    wait: RequestField<bool>,
    #[serde(default)]
    ams_id: RequestField<u32>,
    #[serde(default)]
    slot_id: RequestField<u32>,
    #[serde(default)]
    global_tray_id: RequestField<u32>,
    #[serde(default)]
    external_id: RequestField<String>,
    #[serde(default)]
    duration_hours: RequestField<u16>,
    #[serde(default)]
    filament: RequestField<String>,
    #[serde(default)]
    rotate_tray: RequestField<bool>,
    #[serde(default)]
    holder_action: RequestField<u32>,
    #[serde(default)]
    nozzle_id: RequestField<u32>,
    #[serde(default)]
    extruder_id: RequestField<u32>,
    #[serde(default)]
    light_on: RequestField<bool>,
    #[serde(default)]
    error_action: RequestField<PrintErrorAction>,
    #[serde(default)]
    print_error: RequestField<u32>,
    #[serde(default)]
    printer_job_id: RequestField<String>,
    #[serde(default)]
    sequence_id: RequestField<u64>,
    #[serde(default)]
    error_generation: RequestField<u64>,
    #[serde(default)]
    pub(super) required_device_features: RequestField<Vec<RequiredDeviceFeature>>,
}

pub(in crate::routes) enum PluginPrinterOperation {
    Queued(PrinterOperationKind),
    Live(PrinterOperationKind),
}

pub(in crate::routes) enum TenantPrinterOperation {
    Queued(PrinterOperationKind),
    HandlePrintError {
        error_action: PrintErrorAction,
        error_generation: u64,
    },
}

impl PrinterOperationRequest {
    pub(in crate::routes) fn into_tenant_operation(
        self,
    ) -> Result<TenantPrinterOperation, ApiError> {
        if !self.param.is_missing() {
            return Err(invalid_printer_control());
        }
        let required_device_features = device_features::from_request(&self)?;
        if self.action == "handle_print_error" {
            if !self.no_operation_fields() || !self.no_plugin_transport_fields() {
                return Err(invalid_printer_control());
            }
            let (Some(error_action), Some(error_generation)) = (
                self.error_action.into_option(),
                self.error_generation.into_option(),
            ) else {
                return Err(invalid_printer_control());
            };
            return Ok(TenantPrinterOperation::HandlePrintError {
                error_action,
                error_generation,
            });
        }
        if !self.no_native_fields() {
            return Err(invalid_printer_control());
        }
        if self.action.starts_with("ams_") {
            return self
                .into_ams_operation()
                .map(TenantPrinterOperation::Queued);
        }

        let operation: Result<PrinterOperationKind, ApiError> = match self.action.as_str() {
            "pause" if self.no_operation_fields() => Ok(PrinterOperationKind::Pause {}),
            "resume" if self.no_operation_fields() => Ok(PrinterOperationKind::Resume {}),
            "stop" if self.no_operation_fields() => Ok(PrinterOperationKind::Stop {}),
            "toggle_light" if self.no_operation_fields() => {
                Ok(PrinterOperationKind::ToggleLight {})
            }
            "nozzle_holder_ctrl"
                if self.no_non_rack_fields()
                    && self.nozzle_id.is_missing()
                    && self.holder_action.is_some() =>
            {
                Ok(PrinterOperationKind::NozzleHolderCtrl {
                    action: self.holder_action.expect("checked above"),
                })
            }
            "nozzle_info_confirm"
                if self.no_non_rack_fields()
                    && self.holder_action.is_missing()
                    && self.nozzle_id.is_some() =>
            {
                Ok(PrinterOperationKind::NozzleInfoConfirm {
                    id: self.nozzle_id.expect("checked above"),
                })
            }
            "holder_nozzle_refresh"
                if self.no_non_rack_fields()
                    && self.holder_action.is_missing()
                    && self.nozzle_id.is_some() =>
            {
                Ok(PrinterOperationKind::HolderNozzleRefresh {
                    id: self.nozzle_id.expect("checked above"),
                })
            }
            "set_chamber_light"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_material_fields()
                    && self.extruder_id.is_missing()
                    && self.no_rack_fields()
                    && self.light_on.is_some() =>
            {
                Ok(PrinterOperationKind::SetChamberLight {
                    on: self.light_on.expect("checked above"),
                })
            }
            "set_print_speed"
                if self.speed_mode.is_some()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_ams_fields()
                    && self.no_rack_fields() =>
            {
                Ok(PrinterOperationKind::SetPrintSpeed {
                    speed_mode: self.speed_mode.expect("checked above"),
                })
            }
            "select_extruder"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.ams_id.is_missing()
                    && self.slot_id.is_missing()
                    && self.global_tray_id.is_missing()
                    && self.external_id.is_missing()
                    && self.no_drying_fields()
                    && self.no_rack_fields()
                    && self.extruder_id.is_some() =>
            {
                Ok(PrinterOperationKind::SelectExtruder {
                    extruder_id: self.extruder_id.expect("checked above"),
                })
            }
            "home"
                if self.speed_mode.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_ams_fields()
                    && self.no_rack_fields() =>
            {
                Ok(PrinterOperationKind::Home {
                    axes: self.axes.unwrap_or_default(),
                    required_device_features,
                })
            }
            "move_axes"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.temperature_celsius.is_missing()
                    && self.wait.is_missing()
                    && self.no_ams_fields()
                    && self.no_rack_fields() =>
            {
                Ok(PrinterOperationKind::MoveAxes {
                    movements: self.movements.unwrap_or_default(),
                    feedrate_mm_per_min: self.feedrate_mm_per_min.into_option(),
                    required_device_features,
                })
            }
            "set_hotend_temperature"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.no_material_fields()
                    && self.no_rack_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetHotendTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                    extruder_id: self.extruder_id.into_option(),
                })
            }
            "set_bed_temperature"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.no_ams_fields()
                    && self.no_rack_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetBedTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                })
            }
            "set_chamber_temperature"
                if self.speed_mode.is_missing()
                    && self.axes.is_missing()
                    && self.movements.is_missing()
                    && self.feedrate_mm_per_min.is_missing()
                    && self.no_ams_fields()
                    && self.no_rack_fields()
                    && self.temperature_celsius.is_some() =>
            {
                Ok(PrinterOperationKind::SetChamberTemperature {
                    temperature_celsius: self.temperature_celsius.expect("checked above"),
                    wait: self.wait.unwrap_or(false),
                })
            }
            _ => return Err(invalid_printer_control()),
        };
        let operation = operation?;
        Ok(TenantPrinterOperation::Queued(operation))
    }

    pub(super) fn no_operation_fields(&self) -> bool {
        self.no_non_rack_fields() && self.no_rack_fields()
    }

    fn no_non_rack_fields(&self) -> bool {
        self.speed_mode.is_missing()
            && self.axes.is_missing()
            && self.movements.is_missing()
            && self.feedrate_mm_per_min.is_missing()
            && self.temperature_celsius.is_missing()
            && self.wait.is_missing()
            && self.no_ams_fields()
            && self.light_on.is_missing()
    }

    fn no_rack_fields(&self) -> bool {
        self.holder_action.is_missing() && self.nozzle_id.is_missing()
    }

    fn no_ams_fields(&self) -> bool {
        self.no_material_fields() && self.extruder_id.is_missing() && self.light_on.is_missing()
    }

    fn no_material_fields(&self) -> bool {
        self.ams_id.is_missing()
            && self.slot_id.is_missing()
            && self.global_tray_id.is_missing()
            && self.external_id.is_missing()
            && self.no_drying_fields()
    }

    fn no_drying_fields(&self) -> bool {
        self.duration_hours.is_missing()
            && self.filament.is_missing()
            && self.rotate_tray.is_missing()
    }

    pub(super) fn no_native_fields(&self) -> bool {
        self.error_action.is_missing()
            && self.print_error.is_missing()
            && self.printer_job_id.is_missing()
            && self.sequence_id.is_missing()
            && self.error_generation.is_missing()
    }

    fn no_plugin_transport_fields(&self) -> bool {
        self.print_error.is_missing()
            && self.printer_job_id.is_missing()
            && self.sequence_id.is_missing()
    }
}
