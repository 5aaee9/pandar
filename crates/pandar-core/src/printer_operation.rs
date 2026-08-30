use serde::{Deserialize, Serialize};

use crate::{H2cAutoNozzleMappingRequest, PrintErrorAction, RequiredDeviceFeature};

mod validation;
pub use validation::PrinterOperationValidationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterAxis {
    X,
    Y,
    Z,
}

impl PrinterAxis {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PrinterAxisMovement {
    pub axis: PrinterAxis,
    pub delta_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrinterOperation {
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

impl PrinterOperation {
    pub const fn action(&self) -> &'static str {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_json_round_trips_the_canonical_operation_shape() {
        let operation = PrinterOperation::MoveAxes {
            movements: vec![PrinterAxisMovement {
                axis: PrinterAxis::Y,
                delta_mm: -10.0,
            }],
            feedrate_mm_per_min: None,
            required_device_features: vec![RequiredDeviceFeature::BambuMqttAxisControl],
        };

        let json = serde_json::to_value(&operation).unwrap();
        assert_eq!(json["type"], "move_axes");
        assert_eq!(json["movements"][0]["axis"], "y");
        assert_eq!(
            json["required_device_features"][0],
            "bambu_mqtt_axis_control"
        );
        assert_eq!(
            serde_json::from_value::<PrinterOperation>(json).unwrap(),
            operation
        );
        operation.validate().unwrap();
    }

    #[test]
    fn validation_owns_shared_limits_and_feature_semantics() {
        for invalid_operation in [
            PrinterOperation::SetPrintSpeed { speed_mode: 0 },
            PrinterOperation::MoveAxes {
                movements: vec![
                    PrinterAxisMovement {
                        axis: PrinterAxis::X,
                        delta_mm: 1.0,
                    },
                    PrinterAxisMovement {
                        axis: PrinterAxis::X,
                        delta_mm: 10.0,
                    },
                ],
                feedrate_mm_per_min: None,
                required_device_features: Vec::new(),
            },
            PrinterOperation::Home {
                axes: vec![PrinterAxis::X],
                required_device_features: vec![RequiredDeviceFeature::BambuMqttHoming],
            },
        ] {
            assert!(invalid_operation.validate().is_err());
        }
    }
}
