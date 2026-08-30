pub use pandar_core::PrintErrorAction;
use pandar_core::{H2cAutoNozzleMappingRequest, RequiredDeviceFeature};
use serde::{Deserialize, Serialize};

mod audit;
mod validate;

pub use audit::operation_audit_metadata;
pub use validate::validate_printer_operation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterOperationPayload {
    pub printer_id: String,
    pub serial_number: String,
    pub operation: PrinterOperationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}

impl PrinterAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrinterAxisMovement {
    pub axis: PrinterAxis,
    pub delta_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrinterOperationKind {
    Pause {},
    Resume {},
    Stop {},
    HandlePrintError {
        error_action: PrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
    GcodeLine {
        param: String,
    },
    ToggleLight {},
    SetChamberLight {
        on: bool,
    },
    SetPrintSpeed {
        speed_mode: u8,
    },
    SetFanSpeed {
        fan_index: u8,
        speed_percent: u8,
        airduct: bool,
    },
    SelectExtruder {
        extruder_id: u32,
    },
    Home {
        #[serde(default)]
        axes: Vec<PrinterAxis>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        required_device_features: Vec<RequiredDeviceFeature>,
    },
    MoveAxes {
        movements: Vec<PrinterAxisMovement>,
        #[serde(default)]
        feedrate_mm_per_min: Option<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        required_device_features: Vec<RequiredDeviceFeature>,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: Option<u32>,
    },
    SetBedTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    SetChamberTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    AmsRereadRfid {
        ams_id: u32,
        slot_id: u32,
    },
    AmsLoadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsUnloadFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsStartDrying {
        ams_id: u32,
        temperature_celsius: u16,
        duration_hours: u16,
        filament: String,
        rotate_tray: bool,
    },
    AmsStopDrying {
        ams_id: u32,
    },
    GetAutoNozzleMapping {
        request: H2cAutoNozzleMappingRequest,
    },
    NozzleHolderCtrl {
        action: u32,
    },
    NozzleInfoConfirm {
        id: u32,
    },
    HolderNozzleRefresh {
        id: u32,
    },
}

impl PrinterOperationKind {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Pause {} => "pause",
            Self::Resume {} => "resume",
            Self::Stop {} => "stop",
            Self::HandlePrintError { .. } => "handle_print_error",
            Self::GcodeLine { .. } => "gcode_line",
            Self::ToggleLight {} => "toggle_light",
            Self::SetChamberLight { .. } => "set_chamber_light",
            Self::SetPrintSpeed { .. } => "set_print_speed",
            Self::SetFanSpeed { .. } => "set_fan_speed",
            Self::SelectExtruder { .. } => "select_extruder",
            Self::Home { .. } => "home",
            Self::MoveAxes { .. } => "move_axes",
            Self::SetHotendTemperature { .. } => "set_hotend_temperature",
            Self::SetBedTemperature { .. } => "set_bed_temperature",
            Self::SetChamberTemperature { .. } => "set_chamber_temperature",
            Self::AmsRereadRfid { .. } => "ams_reread_rfid",
            Self::AmsLoadFilament { .. } => "ams_load_filament",
            Self::AmsUnloadFilament { .. } => "ams_unload_filament",
            Self::AmsStartDrying { .. } => "ams_start_drying",
            Self::AmsStopDrying { .. } => "ams_stop_drying",
            Self::GetAutoNozzleMapping { .. } => "get_auto_nozzle_mapping",
            Self::NozzleHolderCtrl { .. } => "nozzle_holder_ctrl",
            Self::NozzleInfoConfirm { .. } => "nozzle_info_confirm",
            Self::HolderNozzleRefresh { .. } => "holder_nozzle_refresh",
        }
    }

    pub fn required_device_features(&self) -> &[RequiredDeviceFeature] {
        match self {
            Self::Home {
                required_device_features,
                ..
            }
            | Self::MoveAxes {
                required_device_features,
                ..
            } => required_device_features,
            _ => &[],
        }
    }

    pub(crate) fn has_valid_required_device_features(&self) -> bool {
        match self {
            Self::Home {
                axes,
                required_device_features,
            } if !required_device_features.is_empty() => {
                axes.is_empty()
                    && required_device_features == &[RequiredDeviceFeature::BambuMqttHoming]
            }
            Self::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            } if !required_device_features.is_empty() => {
                movements.len() == 1
                    && feedrate_mm_per_min.is_none()
                    && matches!(movements[0].delta_mm.abs(), 1.0 | 10.0)
                    && required_device_features == &[RequiredDeviceFeature::BambuMqttAxisControl]
            }
            _ => true,
        }
    }
}
