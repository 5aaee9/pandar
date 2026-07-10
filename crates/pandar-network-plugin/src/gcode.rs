pub(super) use operation::{PrintErrorAction, PrinterOperation};

mod operation;
mod studio_json;

use crate::{PluginHttpResult, result, stable_error_body};
use operation::{Axis, AxisMovement};

const PARSE_OPERATION: i32 = 0;
const PARSE_UNSUPPORTED: i32 = 1;
const PARSE_INVALID_NATIVE: i32 = 2;

pub(crate) enum StudioOperationParse {
    Operation(PrinterOperation),
    Unsupported,
    InvalidNativeCandidate,
}

pub(super) fn operation_json_from_gcode(message: &str) -> StudioOperationParse {
    match studio_json::parse_studio_json_operation(message) {
        StudioOperationParse::Unsupported => {}
        parsed => return parsed,
    }

    let commands = gcode_commands(message);
    let operation = match commands.as_slice() {
        [command] => parse_single_command_operation(command),
        [relative, movement] if relative.eq_ignore_ascii_case("G91") => {
            parse_move_axes_operation(movement)
        }
        _ => None,
    };
    operation.map_or(
        StudioOperationParse::Unsupported,
        StudioOperationParse::Operation,
    )
}

impl StudioOperationParse {
    pub(super) fn into_http_result(self) -> PluginHttpResult {
        match self {
            Self::Operation(operation) => result(
                PARSE_OPERATION,
                200,
                serde_json::to_string(&operation).expect("printer operation is serializable"),
            ),
            Self::Unsupported => result(
                PARSE_UNSUPPORTED,
                400,
                stable_error_body("unsupported_printer_operation"),
            ),
            Self::InvalidNativeCandidate => result(
                PARSE_INVALID_NATIVE,
                400,
                stable_error_body("unsupported_printer_operation"),
            ),
        }
    }
}

fn gcode_commands(message: &str) -> Vec<String> {
    message
        .lines()
        .filter_map(|line| {
            let command = line
                .split_once(';')
                .map_or(line, |(command, _)| command)
                .trim();
            (!command.is_empty()).then(|| command.to_owned())
        })
        .collect()
}

fn parse_single_command_operation(command: &str) -> Option<PrinterOperation> {
    match command_code(command)? {
        "G28" => parse_home_operation(command),
        "M104" => parse_hotend_operation(command, false),
        "M109" => parse_hotend_operation(command, true),
        "M140" => parse_temperature_operation(command, TemperatureOperation::Bed, false),
        "M190" => parse_temperature_operation(command, TemperatureOperation::Bed, true),
        "M141" => parse_temperature_operation(command, TemperatureOperation::Chamber, false),
        "M191" => parse_temperature_operation(command, TemperatureOperation::Chamber, true),
        _ => None,
    }
}

fn parse_home_operation(command: &str) -> Option<PrinterOperation> {
    let mut axes = Vec::new();
    for token in command.split_whitespace().skip(1) {
        let axis = match token.to_ascii_uppercase().as_str() {
            "X" => Axis::X,
            "Y" => Axis::Y,
            "Z" => Axis::Z,
            _ => return None,
        };
        axes.push(axis);
    }
    Some(PrinterOperation::Home { axes: Some(axes) })
}

fn parse_hotend_operation(command: &str, wait: bool) -> Option<PrinterOperation> {
    parse_temperature_operation(command, TemperatureOperation::Hotend, wait)
}

fn parse_temperature_operation(
    command: &str,
    operation: TemperatureOperation,
    wait: bool,
) -> Option<PrinterOperation> {
    let mut celsius = None;
    let mut extruder_id = None;
    for token in command.split_whitespace().skip(1) {
        let mut chars = token.chars();
        let parameter = chars.next()?.to_ascii_uppercase();
        let value = parse_gcode_number(chars.as_str())?;
        match parameter {
            'S' if celsius.is_none() => celsius = Some(value),
            'T' if operation == TemperatureOperation::Hotend && extruder_id.is_none() => {
                extruder_id = Some(value);
            }
            _ => return None,
        }
    }
    let celsius = parse_integer_gcode_value(celsius?)?;
    let operation = match operation {
        TemperatureOperation::Hotend => PrinterOperation::SetHotendTemperature {
            temperature_celsius: celsius,
            wait: Some(wait),
            extruder_id: match extruder_id {
                Some(value) => Some(parse_integer_gcode_value(value)?),
                None => None,
            },
        },
        TemperatureOperation::Bed => PrinterOperation::SetBedTemperature {
            temperature_celsius: celsius,
            wait: Some(wait),
        },
        TemperatureOperation::Chamber => PrinterOperation::SetChamberTemperature {
            temperature_celsius: celsius,
            wait: Some(wait),
        },
    };
    operation.is_valid().then_some(operation)
}

fn parse_move_axes_operation(command: &str) -> Option<PrinterOperation> {
    if !matches!(command_code(command)?, "G0" | "G1") {
        return None;
    }

    let mut movements = Vec::new();
    let mut feedrate = None;
    for token in command.split_whitespace().skip(1) {
        let mut chars = token.chars();
        let parameter = chars.next()?.to_ascii_uppercase();
        let value = parse_gcode_number(chars.as_str())?;
        match parameter {
            'X' | 'Y' | 'Z'
                if !movements.iter().any(|movement: &AxisMovement| {
                    movement.axis == axis_from_parameter(parameter)
                }) =>
            {
                movements.push(AxisMovement {
                    axis: axis_from_parameter(parameter),
                    delta_mm: value,
                });
            }
            'F' if feedrate.is_none() => feedrate = Some(value),
            _ => return None,
        }
    }
    if movements.is_empty() {
        return None;
    }

    let operation = PrinterOperation::MoveAxes {
        movements,
        feedrate_mm_per_min: match feedrate {
            Some(value) => Some(parse_integer_gcode_value(value)?),
            None => None,
        },
    };
    operation.is_valid().then_some(operation)
}

fn command_code(command: &str) -> Option<&str> {
    command
        .split_whitespace()
        .next()
        .map(|code| code.trim())
        .filter(|code| !code.is_empty())
        .and_then(|code| {
            if code.eq_ignore_ascii_case("G0") {
                Some("G0")
            } else if code.eq_ignore_ascii_case("G1") {
                Some("G1")
            } else if code.eq_ignore_ascii_case("G28") {
                Some("G28")
            } else if code.eq_ignore_ascii_case("G90") {
                Some("G90")
            } else if code.eq_ignore_ascii_case("G91") {
                Some("G91")
            } else if code.eq_ignore_ascii_case("M104") {
                Some("M104")
            } else if code.eq_ignore_ascii_case("M109") {
                Some("M109")
            } else if code.eq_ignore_ascii_case("M140") {
                Some("M140")
            } else if code.eq_ignore_ascii_case("M190") {
                Some("M190")
            } else if code.eq_ignore_ascii_case("M141") {
                Some("M141")
            } else if code.eq_ignore_ascii_case("M191") {
                Some("M191")
            } else {
                None
            }
        })
}

fn parse_gcode_number(value: &str) -> Option<f64> {
    (!value.is_empty())
        .then(|| value.parse::<f64>().ok())
        .flatten()
        .filter(|value| value.is_finite())
}

fn parse_integer_gcode_value(value: f64) -> Option<u64> {
    (value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f64).then_some(value as u64)
}

fn axis_from_parameter(parameter: char) -> Axis {
    match parameter {
        'X' => Axis::X,
        'Y' => Axis::Y,
        'Z' => Axis::Z,
        _ => unreachable!("axis parameter is matched before conversion"),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TemperatureOperation {
    Hotend,
    Bed,
    Chamber,
}
