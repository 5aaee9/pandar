use pandar_core::{
    PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperation, RequiredDeviceFeature,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PrinterOperationRequest {
    Pause,
    Resume,
    Stop,
    GcodeLine {
        param: String,
    },
    HandlePrintError {
        error_action: PrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
    ToggleLight,
    SetChamberLight {
        light_on: bool,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        axes: Option<Vec<PrinterAxis>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        required_device_features: Vec<RequiredDeviceFeature>,
    },
    MoveAxes {
        movements: Vec<PrinterAxisMovement>,
        #[serde(skip_serializing_if = "Option::is_none")]
        feedrate_mm_per_min: Option<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        required_device_features: Vec<RequiredDeviceFeature>,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extruder_id: Option<u32>,
    },
    SetBedTemperature {
        temperature_celsius: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<bool>,
    },
    SetChamberTemperature {
        temperature_celsius: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait: Option<bool>,
    },
    AmsRereadRfid {
        ams_id: u32,
        slot_id: u32,
    },
    AmsLoadFilament {
        ams_id: u32,
        slot_id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        global_tray_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        external_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extruder_id: Option<u32>,
    },
    AmsUnloadFilament {
        ams_id: u32,
        slot_id: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        global_tray_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        external_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
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
    NozzleHolderCtrl {
        holder_action: u32,
    },
    NozzleInfoConfirm {
        nozzle_id: u32,
    },
    HolderNozzleRefresh {
        nozzle_id: u32,
    },
}

pub(super) fn parse_validated_request(body: &str) -> Option<PrinterOperationRequest> {
    let request = serde_json::from_str::<PrinterOperationRequest>(body).ok()?;
    PrinterOperation::from(request.clone()).validate().ok()?;
    Some(request)
}

pub(super) fn request_json(operation: PrinterOperation) -> Option<String> {
    let request = PrinterOperationRequest::try_from(operation).ok()?;
    Some(serde_json::to_string(&request).expect("printer operation request is serializable"))
}

impl From<PrinterOperationRequest> for PrinterOperation {
    fn from(value: PrinterOperationRequest) -> Self {
        match value {
            PrinterOperationRequest::Pause => Self::Pause {},
            PrinterOperationRequest::Resume => Self::Resume {},
            PrinterOperationRequest::Stop => Self::Stop {},
            PrinterOperationRequest::GcodeLine { param } => Self::GcodeLine { param },
            PrinterOperationRequest::HandlePrintError {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            } => Self::HandlePrintError {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            },
            PrinterOperationRequest::ToggleLight => Self::ToggleLight {},
            PrinterOperationRequest::SetChamberLight { light_on } => {
                Self::SetChamberLight { on: light_on }
            }
            PrinterOperationRequest::SetPrintSpeed { speed_mode } => {
                Self::SetPrintSpeed { speed_mode }
            }
            PrinterOperationRequest::SetFanSpeed {
                fan_index,
                speed_percent,
                airduct,
            } => Self::SetFanSpeed {
                fan_index,
                speed_percent,
                airduct,
            },
            PrinterOperationRequest::SelectExtruder { extruder_id } => {
                Self::SelectExtruder { extruder_id }
            }
            PrinterOperationRequest::Home {
                axes,
                required_device_features,
            } => Self::Home {
                axes: axes.unwrap_or_default(),
                required_device_features,
            },
            PrinterOperationRequest::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            } => Self::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            },
            PrinterOperationRequest::SetHotendTemperature {
                temperature_celsius,
                wait,
                extruder_id,
            } => Self::SetHotendTemperature {
                temperature_celsius,
                wait: wait.unwrap_or(false),
                extruder_id,
            },
            PrinterOperationRequest::SetBedTemperature {
                temperature_celsius,
                wait,
            } => Self::SetBedTemperature {
                temperature_celsius,
                wait: wait.unwrap_or(false),
            },
            PrinterOperationRequest::SetChamberTemperature {
                temperature_celsius,
                wait,
            } => Self::SetChamberTemperature {
                temperature_celsius,
                wait: wait.unwrap_or(false),
            },
            PrinterOperationRequest::AmsRereadRfid { ams_id, slot_id } => {
                Self::AmsRereadRfid { ams_id, slot_id }
            }
            PrinterOperationRequest::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            } => Self::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            },
            PrinterOperationRequest::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            } => Self::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            },
            PrinterOperationRequest::AmsStartDrying {
                ams_id,
                temperature_celsius,
                duration_hours,
                filament,
                rotate_tray,
            } => Self::AmsStartDrying {
                ams_id,
                temperature_celsius,
                duration_hours,
                filament,
                rotate_tray,
            },
            PrinterOperationRequest::AmsStopDrying { ams_id } => Self::AmsStopDrying { ams_id },
            PrinterOperationRequest::NozzleHolderCtrl { holder_action } => Self::NozzleHolderCtrl {
                action: holder_action,
            },
            PrinterOperationRequest::NozzleInfoConfirm { nozzle_id } => {
                Self::NozzleInfoConfirm { id: nozzle_id }
            }
            PrinterOperationRequest::HolderNozzleRefresh { nozzle_id } => {
                Self::HolderNozzleRefresh { id: nozzle_id }
            }
        }
    }
}

impl TryFrom<PrinterOperation> for PrinterOperationRequest {
    type Error = ();

    fn try_from(value: PrinterOperation) -> Result<Self, Self::Error> {
        Ok(match value {
            PrinterOperation::Pause {} => Self::Pause,
            PrinterOperation::Resume {} => Self::Resume,
            PrinterOperation::Stop {} => Self::Stop,
            PrinterOperation::GcodeLine { param } => Self::GcodeLine { param },
            PrinterOperation::HandlePrintError {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            } => Self::HandlePrintError {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            },
            PrinterOperation::ToggleLight {} => Self::ToggleLight,
            PrinterOperation::SetChamberLight { on } => Self::SetChamberLight { light_on: on },
            PrinterOperation::SetPrintSpeed { speed_mode } => Self::SetPrintSpeed { speed_mode },
            PrinterOperation::SetFanSpeed {
                fan_index,
                speed_percent,
                airduct,
            } => Self::SetFanSpeed {
                fan_index,
                speed_percent,
                airduct,
            },
            PrinterOperation::SelectExtruder { extruder_id } => {
                Self::SelectExtruder { extruder_id }
            }
            PrinterOperation::Home {
                axes,
                required_device_features,
            } => Self::Home {
                axes: Some(axes),
                required_device_features,
            },
            PrinterOperation::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            } => Self::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            },
            PrinterOperation::SetHotendTemperature {
                temperature_celsius,
                wait,
                extruder_id,
            } => Self::SetHotendTemperature {
                temperature_celsius,
                wait: Some(wait),
                extruder_id,
            },
            PrinterOperation::SetBedTemperature {
                temperature_celsius,
                wait,
            } => Self::SetBedTemperature {
                temperature_celsius,
                wait: Some(wait),
            },
            PrinterOperation::SetChamberTemperature {
                temperature_celsius,
                wait,
            } => Self::SetChamberTemperature {
                temperature_celsius,
                wait: Some(wait),
            },
            PrinterOperation::AmsRereadRfid { ams_id, slot_id } => {
                Self::AmsRereadRfid { ams_id, slot_id }
            }
            PrinterOperation::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            } => Self::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            },
            PrinterOperation::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            } => Self::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            },
            PrinterOperation::AmsStartDrying {
                ams_id,
                temperature_celsius,
                duration_hours,
                filament,
                rotate_tray,
            } => Self::AmsStartDrying {
                ams_id,
                temperature_celsius,
                duration_hours,
                filament,
                rotate_tray,
            },
            PrinterOperation::AmsStopDrying { ams_id } => Self::AmsStopDrying { ams_id },
            PrinterOperation::NozzleHolderCtrl { action } => Self::NozzleHolderCtrl {
                holder_action: action,
            },
            PrinterOperation::NozzleInfoConfirm { id } => Self::NozzleInfoConfirm { nozzle_id: id },
            PrinterOperation::HolderNozzleRefresh { id } => {
                Self::HolderNozzleRefresh { nozzle_id: id }
            }
            PrinterOperation::GetAutoNozzleMapping { .. } => return Err(()),
        })
    }
}
