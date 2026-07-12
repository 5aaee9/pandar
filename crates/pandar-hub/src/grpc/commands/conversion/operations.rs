use crate::{
    protocol::agent::v1::{
        AmsLoadFilamentOperation, AmsRereadRfidOperation, AmsUnloadFilamentOperation, Axis,
        AxisMovement, GcodeLineOperation, HandlePrintErrorOperation, HomeOperation,
        MoveAxesOperation, PauseOperation, PrintErrorAction as ProtoPrintErrorAction,
        ResumeOperation, SelectExtruderOperation, SetBedTemperatureOperation,
        SetChamberLightOperation, SetChamberTemperatureOperation, SetHotendTemperatureOperation,
        SetPrintSpeedOperation, StopOperation, ToggleLightOperation, printer_operation,
    },
    repositories::{PrintErrorAction, PrinterAxis, PrinterAxisMovement, PrinterOperationKind},
};

pub(super) fn proto_printer_operation(
    operation: PrinterOperationKind,
) -> printer_operation::Operation {
    match operation {
        PrinterOperationKind::Pause {} => printer_operation::Operation::Pause(PauseOperation {}),
        PrinterOperationKind::Resume {} => printer_operation::Operation::Resume(ResumeOperation {}),
        PrinterOperationKind::Stop {} => printer_operation::Operation::Stop(StopOperation {}),
        PrinterOperationKind::HandlePrintError {
            error_action,
            print_error,
            printer_job_id,
            sequence_id,
        } => printer_operation::Operation::HandlePrintError(HandlePrintErrorOperation {
            error_action: proto_print_error_action(error_action) as i32,
            print_error,
            printer_job_id,
            sequence_id,
        }),
        PrinterOperationKind::GcodeLine { param } => {
            printer_operation::Operation::GcodeLine(GcodeLineOperation { param })
        }
        PrinterOperationKind::ToggleLight {} => {
            printer_operation::Operation::ToggleLight(ToggleLightOperation {})
        }
        PrinterOperationKind::SetChamberLight { on } => {
            printer_operation::Operation::SetChamberLight(SetChamberLightOperation { on })
        }
        PrinterOperationKind::SetPrintSpeed { speed_mode } => {
            printer_operation::Operation::SetPrintSpeed(SetPrintSpeedOperation {
                speed_mode: speed_mode.into(),
            })
        }
        PrinterOperationKind::SelectExtruder { extruder_id } => {
            printer_operation::Operation::SelectExtruder(SelectExtruderOperation { extruder_id })
        }
        PrinterOperationKind::Home { axes, .. } => {
            printer_operation::Operation::Home(HomeOperation {
                axes: axes.into_iter().map(proto_axis).collect(),
            })
        }
        PrinterOperationKind::MoveAxes {
            movements,
            feedrate_mm_per_min,
            ..
        } => printer_operation::Operation::MoveAxes(MoveAxesOperation {
            movements: movements.into_iter().map(proto_axis_movement).collect(),
            feedrate_mm_per_min: feedrate_mm_per_min.unwrap_or_default(),
        }),
        PrinterOperationKind::SetHotendTemperature {
            temperature_celsius,
            wait,
            extruder_id,
        } => printer_operation::Operation::SetHotendTemperature(SetHotendTemperatureOperation {
            temperature_celsius: temperature_celsius.into(),
            wait,
            extruder_id,
        }),
        PrinterOperationKind::SetBedTemperature {
            temperature_celsius,
            wait,
        } => printer_operation::Operation::SetBedTemperature(SetBedTemperatureOperation {
            temperature_celsius: temperature_celsius.into(),
            wait,
        }),
        PrinterOperationKind::SetChamberTemperature {
            temperature_celsius,
            wait,
        } => printer_operation::Operation::SetChamberTemperature(SetChamberTemperatureOperation {
            temperature_celsius: temperature_celsius.into(),
            wait,
        }),
        PrinterOperationKind::AmsRereadRfid { ams_id, slot_id } => {
            printer_operation::Operation::AmsRereadRfid(AmsRereadRfidOperation { ams_id, slot_id })
        }
        PrinterOperationKind::AmsLoadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => printer_operation::Operation::AmsLoadFilament(AmsLoadFilamentOperation {
            ams_id,
            slot_id,
            global_tray_id: global_tray_id.unwrap_or_else(|| ams_id * 4 + slot_id),
            external_id: external_id.unwrap_or_default(),
            extruder_id,
        }),
        PrinterOperationKind::AmsUnloadFilament {
            ams_id,
            slot_id,
            global_tray_id,
            external_id,
            extruder_id,
        } => printer_operation::Operation::AmsUnloadFilament(AmsUnloadFilamentOperation {
            ams_id,
            slot_id,
            global_tray_id: global_tray_id.unwrap_or_else(|| ams_id * 4 + slot_id),
            external_id: external_id.unwrap_or_default(),
            extruder_id,
        }),
    }
}

fn proto_print_error_action(action: PrintErrorAction) -> ProtoPrintErrorAction {
    match action {
        PrintErrorAction::Resume => ProtoPrintErrorAction::Resume,
        PrintErrorAction::Ignore => ProtoPrintErrorAction::Ignore,
        PrintErrorAction::Stop => ProtoPrintErrorAction::Stop,
    }
}

fn proto_axis(axis: PrinterAxis) -> i32 {
    match axis {
        PrinterAxis::X => Axis::X as i32,
        PrinterAxis::Y => Axis::Y as i32,
        PrinterAxis::Z => Axis::Z as i32,
    }
}

fn proto_axis_movement(movement: PrinterAxisMovement) -> AxisMovement {
    AxisMovement {
        axis: proto_axis(movement.axis),
        delta_mm: movement.delta_mm,
    }
}
