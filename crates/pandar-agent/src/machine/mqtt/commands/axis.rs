use serde::Serialize;

use crate::machine::PrinterAxis;

use super::{BambuMqttCommandPayload, next_studio_sequence_id, payload::json_payload};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcodeLineCommand {
    pub lines: Vec<String>,
}

pub(super) fn back_to_center_payload() -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(AxisPrintPayload {
            print: BackToCenterPayload {
                command: "back_to_center",
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

pub(super) fn xyz_control_payload(
    axis: PrinterAxis,
    direction: i8,
    mode: u8,
) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(AxisPrintPayload {
            print: XyzControlPayload {
                command: "xyz_ctrl",
                axis: axis_name(axis),
                direction,
                mode,
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

pub(super) fn gcode_line_payload(command: &GcodeLineCommand) -> BambuMqttCommandPayload {
    let sequence_id = next_studio_sequence_id();
    BambuMqttCommandPayload::with_sequence(
        json_payload(AxisPrintPayload {
            print: GcodeLinePayload {
                command: "gcode_line",
                param: command.lines.join("\n"),
                sequence_id: sequence_id.clone(),
            },
        }),
        sequence_id,
    )
}

fn axis_name(axis: PrinterAxis) -> &'static str {
    match axis {
        PrinterAxis::X => "X",
        PrinterAxis::Y => "Y",
        PrinterAxis::Z => "Z",
    }
}

#[derive(Serialize)]
struct AxisPrintPayload<T> {
    print: T,
}

#[derive(Serialize)]
struct BackToCenterPayload {
    command: &'static str,
    sequence_id: String,
}

#[derive(Serialize)]
struct XyzControlPayload {
    command: &'static str,
    axis: &'static str,
    #[serde(rename = "dir")]
    direction: i8,
    mode: u8,
    sequence_id: String,
}

#[derive(Serialize)]
struct GcodeLinePayload {
    command: &'static str,
    param: String,
    sequence_id: String,
}
