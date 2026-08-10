use serde::Serialize;

use super::{PrintErrorAction, PrinterAxis, PrinterOperationKind};
use crate::grpc::commands::RequiredDeviceFeature;
use crate::repositories::audit::{AuditMetadata, audit_metadata};

pub fn operation_audit_metadata(
    agent_id: String,
    serial_number: String,
    operation: &PrinterOperationKind,
) -> AuditMetadata {
    audit_metadata(OperationAuditMetadata {
        agent_id,
        serial_number,
        action: operation.action(),
        fields: OperationAuditFields::from(operation),
    })
}

#[derive(Serialize)]
struct OperationAuditMetadata {
    agent_id: String,
    serial_number: String,
    action: &'static str,
    #[serde(flatten)]
    fields: OperationAuditFields,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OperationAuditFields {
    Empty {},
    PrintError {
        error_action: PrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
    PrintSpeed {
        speed_mode: u8,
    },
    FanSpeed {
        fan_index: u8,
        speed_percent: u8,
        airduct: bool,
    },
    Extruder {
        extruder_id: u32,
    },
    Home {
        axes: Vec<&'static str>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        required_device_features: Vec<RequiredDeviceFeature>,
    },
    MoveAxes {
        movements: Vec<OperationAuditMovement>,
        feedrate_mm_per_min: Option<u32>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        required_device_features: Vec<RequiredDeviceFeature>,
    },
    HotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: Option<u32>,
    },
    Temperature {
        temperature_celsius: u16,
        wait: bool,
    },
    AmsSlot {
        ams_id: u32,
        slot_id: u32,
    },
    ChamberLight {
        light_on: bool,
    },
    AmsFilament {
        ams_id: u32,
        slot_id: u32,
        global_tray_id: Option<u32>,
        external_id: Option<String>,
        extruder_id: Option<u32>,
    },
    AmsDrying {
        ams_id: u32,
        temperature_celsius: u16,
        duration_hours: u16,
        filament: String,
        rotate_tray: bool,
    },
    AmsUnit {
        ams_id: u32,
    },
    HolderCtrl {
        holder_action: u32,
    },
    RackNozzle {
        nozzle_id: u32,
    },
}

#[derive(Serialize)]
struct OperationAuditMovement {
    axis: &'static str,
    delta_mm: f64,
}

impl OperationAuditFields {
    fn from(operation: &PrinterOperationKind) -> Self {
        match operation {
            PrinterOperationKind::HandlePrintError {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            } => Self::PrintError {
                error_action: *error_action,
                print_error: *print_error,
                printer_job_id: printer_job_id.clone(),
                sequence_id: *sequence_id,
            },
            PrinterOperationKind::SetPrintSpeed { speed_mode } => Self::PrintSpeed {
                speed_mode: *speed_mode,
            },
            PrinterOperationKind::SetFanSpeed {
                fan_index,
                speed_percent,
                airduct,
            } => Self::FanSpeed {
                fan_index: *fan_index,
                speed_percent: *speed_percent,
                airduct: *airduct,
            },
            PrinterOperationKind::SelectExtruder { extruder_id } => Self::Extruder {
                extruder_id: *extruder_id,
            },
            PrinterOperationKind::Home {
                axes,
                required_device_features,
            } => Self::Home {
                axes: axis_names(axes),
                required_device_features: required_device_features.clone(),
            },
            PrinterOperationKind::MoveAxes {
                movements,
                feedrate_mm_per_min,
                required_device_features,
            } => Self::MoveAxes {
                movements: movements
                    .iter()
                    .map(|movement| OperationAuditMovement {
                        axis: movement.axis.as_str(),
                        delta_mm: movement.delta_mm,
                    })
                    .collect(),
                feedrate_mm_per_min: *feedrate_mm_per_min,
                required_device_features: required_device_features.clone(),
            },
            PrinterOperationKind::SetHotendTemperature {
                temperature_celsius,
                wait,
                extruder_id,
            } => Self::HotendTemperature {
                temperature_celsius: *temperature_celsius,
                wait: *wait,
                extruder_id: *extruder_id,
            },
            PrinterOperationKind::SetBedTemperature {
                temperature_celsius,
                wait,
            }
            | PrinterOperationKind::SetChamberTemperature {
                temperature_celsius,
                wait,
            } => Self::Temperature {
                temperature_celsius: *temperature_celsius,
                wait: *wait,
            },
            PrinterOperationKind::AmsRereadRfid { ams_id, slot_id } => Self::AmsSlot {
                ams_id: *ams_id,
                slot_id: *slot_id,
            },
            PrinterOperationKind::SetChamberLight { on } => Self::ChamberLight { light_on: *on },
            PrinterOperationKind::AmsLoadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            }
            | PrinterOperationKind::AmsUnloadFilament {
                ams_id,
                slot_id,
                global_tray_id,
                external_id,
                extruder_id,
            } => Self::AmsFilament {
                ams_id: *ams_id,
                slot_id: *slot_id,
                global_tray_id: *global_tray_id,
                external_id: external_id.clone(),
                extruder_id: *extruder_id,
            },
            PrinterOperationKind::NozzleHolderCtrl { action } => Self::HolderCtrl {
                holder_action: *action,
            },
            PrinterOperationKind::AmsStartDrying {
                ams_id,
                temperature_celsius,
                duration_hours,
                filament,
                rotate_tray,
            } => Self::AmsDrying {
                ams_id: *ams_id,
                temperature_celsius: *temperature_celsius,
                duration_hours: *duration_hours,
                filament: filament.clone(),
                rotate_tray: *rotate_tray,
            },
            PrinterOperationKind::AmsStopDrying { ams_id } => Self::AmsUnit { ams_id: *ams_id },
            PrinterOperationKind::NozzleInfoConfirm { id }
            | PrinterOperationKind::HolderNozzleRefresh { id } => {
                Self::RackNozzle { nozzle_id: *id }
            }
            PrinterOperationKind::Pause {}
            | PrinterOperationKind::Resume {}
            | PrinterOperationKind::Stop {}
            | PrinterOperationKind::ToggleLight {}
            | PrinterOperationKind::GcodeLine { .. }
            | PrinterOperationKind::GetAutoNozzleMapping { .. } => Self::Empty {},
        }
    }
}

fn axis_names(axes: &[PrinterAxis]) -> Vec<&'static str> {
    axes.iter().map(|axis| axis.as_str()).collect()
}
