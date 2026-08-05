use pandar_core::BambuDeviceFeatures;

use super::{PrinterOperation, axis};
use crate::machine::mqtt::{
    AmsDryingCommand, AmsFilamentCommand, AmsSlotCommand, BambuMqttCommand, GcodeLineCommand,
    HandlePrintErrorCommand, PrintSpeed, SetNozzleTemperatureCommand,
};

pub(in crate::machine) fn mqtt_command_for_printer_operation(
    operation: PrinterOperation,
) -> anyhow::Result<BambuMqttCommand> {
    mqtt_command_for_printer_operation_with_features(operation, None)
}

pub(super) fn mqtt_command_for_printer_operation_with_features(
    operation: PrinterOperation,
    observed_features: Option<BambuDeviceFeatures>,
) -> anyhow::Result<BambuMqttCommand> {
    match operation {
        PrinterOperation::Pause => Ok(BambuMqttCommand::PausePrint),
        PrinterOperation::Resume => Ok(BambuMqttCommand::ResumePrint),
        PrinterOperation::Stop => Ok(BambuMqttCommand::StopPrint),
        PrinterOperation::HandlePrintError {
            error_action,
            print_error,
            printer_job_id,
            sequence_id,
        } => Ok(BambuMqttCommand::HandlePrintError(
            HandlePrintErrorCommand {
                error_action,
                print_error,
                printer_job_id,
                sequence_id,
            },
        )),
        PrinterOperation::ToggleLight => {
            unreachable!("toggle_light is handled before payload mapping")
        }
        PrinterOperation::SetChamberLight(_) => {
            unreachable!("set_chamber_light is handled before payload mapping")
        }
        PrinterOperation::SetPrintSpeed(mode) => {
            Ok(BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(mode)?))
        }
        PrinterOperation::GcodeLine { param } => {
            Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand { param }))
        }
        PrinterOperation::GetAutoNozzleMapping(request) => {
            Ok(BambuMqttCommand::GetAutoNozzleMapping(request))
        }
        PrinterOperation::NozzleHolderCtrl { action } => {
            Ok(BambuMqttCommand::NozzleHolderCtrl(action))
        }
        PrinterOperation::NozzleInfoConfirm { id } => Ok(BambuMqttCommand::NozzleInfoConfirm(id)),
        PrinterOperation::HolderNozzleRefresh { id } => {
            Ok(BambuMqttCommand::HolderNozzleRefresh(id))
        }
        PrinterOperation::SelectExtruder(extruder_id) => {
            Ok(BambuMqttCommand::SelectExtruder(extruder_id))
        }
        PrinterOperation::Home {
            axes,
            required_feature,
        } => axis::home_command(axes, required_feature, observed_features),
        PrinterOperation::MoveAxes {
            x_mm,
            y_mm,
            z_mm,
            feedrate_mm_per_min,
            required_feature,
        } => axis::move_axes_command(
            x_mm,
            y_mm,
            z_mm,
            feedrate_mm_per_min,
            required_feature,
            observed_features,
        ),
        PrinterOperation::SetHotendTemperature {
            temperature_celsius,
            wait,
            extruder_id,
        } => match extruder_id {
            Some(extruder_id) => Ok(BambuMqttCommand::SetNozzleTemperature(
                SetNozzleTemperatureCommand {
                    extruder_id,
                    target_temp: temperature_celsius,
                },
            )),
            None => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
                param: format!(
                    "{} S{}",
                    if wait { "M109" } else { "M104" },
                    temperature_celsius
                ),
            })),
        },
        PrinterOperation::SetBedTemperature {
            temperature_celsius,
            wait,
        } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            param: format!(
                "{} S{}",
                if wait { "M190" } else { "M140" },
                temperature_celsius
            ),
        })),
        PrinterOperation::SetChamberTemperature {
            temperature_celsius,
            wait,
        } => Ok(BambuMqttCommand::GcodeLine(GcodeLineCommand {
            param: if wait {
                [
                    "M106 P2 S255".to_string(),
                    format!("M191 S{}", temperature_celsius),
                    "M106 P2 S0".to_string(),
                ]
                .join("\n")
            } else {
                format!("M141 S{}", temperature_celsius)
            },
        })),
        PrinterOperation::AmsRereadRfid { ams_id, slot_id } => {
            Ok(BambuMqttCommand::AmsRereadRfid(AmsSlotCommand {
                ams_id,
                slot_id,
            }))
        }
        PrinterOperation::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => Ok(BambuMqttCommand::AmsLoadFilament(AmsFilamentCommand {
            ams_id: ams_command_ams_id(ams_id, external_id.as_deref()),
            slot_id: ams_command_slot_id(slot_id, external_id.as_deref()),
            target: global_tray_id.unwrap_or(slot_id),
            extruder_id,
        })),
        PrinterOperation::AmsUnloadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => Ok(BambuMqttCommand::AmsUnloadFilament(AmsFilamentCommand {
            ams_id: ams_command_ams_id(ams_id, external_id.as_deref()),
            slot_id: ams_command_slot_id(slot_id, external_id.as_deref()),
            target: global_tray_id.unwrap_or(slot_id),
            extruder_id,
        })),
        PrinterOperation::AmsStartDrying {
            ams_id,
            temperature_celsius,
            duration_hours,
            filament,
            rotate_tray,
        } => Ok(BambuMqttCommand::AmsStartDrying(AmsDryingCommand {
            ams_id,
            temperature_celsius,
            duration_hours,
            filament,
            rotate_tray,
        })),
        PrinterOperation::AmsStopDrying { ams_id } => Ok(BambuMqttCommand::AmsStopDrying(ams_id)),
    }
}

fn ams_command_ams_id(ams_id: u32, external_id: Option<&str>) -> u32 {
    if external_id.is_some() { 255 } else { ams_id }
}

fn ams_command_slot_id(slot_id: u32, external_id: Option<&str>) -> u32 {
    if matches!(external_id, Some("254")) {
        254
    } else {
        slot_id
    }
}
