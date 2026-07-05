use serde_json::{Value, json};

mod studio_json;

const MAX_EXTRUDER_ID: u64 = 1;
const MAX_HOTEND_TEMPERATURE_CELSIUS: u64 = 300;
const MAX_BED_TEMPERATURE_CELSIUS: u64 = 120;
const MAX_CHAMBER_TEMPERATURE_CELSIUS: u64 = 70;
const MAX_AMS_ID: u64 = 255;
const MAX_AMS_SLOT_ID: u64 = 255;
const MAX_U32: u64 = u32::MAX as u64;

pub(super) fn operation_json_from_gcode(message: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(message)
        && let Some(operation) = studio_json::parse_studio_json_operation(&value)
    {
        return Some(operation);
    }

    let commands = gcode_commands(message);
    match commands.as_slice() {
        [command] => parse_single_command_operation(command),
        [relative, movement] if relative.eq_ignore_ascii_case("G91") => {
            parse_move_axes_operation(movement)
        }
        _ => None,
    }
}

pub(super) fn valid_operation_json(operation: &Value) -> bool {
    let Some(object) = operation.as_object() else {
        return false;
    };
    let Some(action) = object.get("action").and_then(Value::as_str) else {
        return false;
    };

    match action {
        "pause" | "resume" | "stop" | "toggle_light" => object.len() == 1,
        "set_chamber_light" => {
            operation.get("light_on").is_some_and(Value::is_boolean) && object.len() == 2
        }
        "set_print_speed" => valid_u64_field(operation, "speed_mode", 1, 4) && object.len() == 2,
        "select_extruder" => {
            valid_u64_field(operation, "extruder_id", 0, MAX_EXTRUDER_ID) && object.len() == 2
        }
        "home" => {
            operation.get("axes").is_none_or(|axes| {
                axes.as_array()
                    .is_some_and(|axes| axes.iter().all(valid_axis_value))
            }) && object.len() == 1 + usize::from(object.contains_key("axes"))
        }
        "move_axes" => {
            let Some(movements) = operation.get("movements").and_then(Value::as_array) else {
                return false;
            };
            !movements.is_empty()
                && movements.iter().all(valid_movement_value)
                && operation
                    .get("feedrate_mm_per_min")
                    .is_none_or(valid_feedrate_value)
                && object.len() == 2 + usize::from(object.contains_key("feedrate_mm_per_min"))
        }
        "set_hotend_temperature" => {
            valid_u64_field(
                operation,
                "temperature_celsius",
                0,
                MAX_HOTEND_TEMPERATURE_CELSIUS,
            ) && operation.get("wait").is_none_or(Value::is_boolean)
                && valid_optional_u64_field(operation, "extruder_id", 0, MAX_EXTRUDER_ID)
                && object.len()
                    == 2 + usize::from(object.contains_key("wait"))
                        + usize::from(object.contains_key("extruder_id"))
        }
        "set_bed_temperature" => {
            valid_u64_field(
                operation,
                "temperature_celsius",
                0,
                MAX_BED_TEMPERATURE_CELSIUS,
            ) && operation.get("wait").is_none_or(Value::is_boolean)
                && object.len() == 2 + usize::from(object.contains_key("wait"))
        }
        "set_chamber_temperature" => {
            valid_u64_field(
                operation,
                "temperature_celsius",
                0,
                MAX_CHAMBER_TEMPERATURE_CELSIUS,
            ) && operation.get("wait").is_none_or(Value::is_boolean)
                && object.len() == 2 + usize::from(object.contains_key("wait"))
        }
        "ams_reread_rfid" => {
            valid_u64_field(operation, "ams_id", 0, MAX_AMS_ID)
                && valid_u64_field(operation, "slot_id", 0, MAX_AMS_SLOT_ID)
                && object.len() == 3
        }
        "ams_load_filament" | "ams_unload_filament" => {
            valid_u64_field(operation, "ams_id", 0, MAX_AMS_ID)
                && valid_u64_field(operation, "slot_id", 0, MAX_AMS_SLOT_ID)
                && valid_optional_u64_field(operation, "global_tray_id", 0, MAX_U32)
                && operation.get("external_id").is_none_or(Value::is_string)
                && valid_optional_u64_field(operation, "extruder_id", 0, MAX_EXTRUDER_ID)
                && object.len()
                    == 3 + usize::from(object.contains_key("global_tray_id"))
                        + usize::from(object.contains_key("external_id"))
                        + usize::from(object.contains_key("extruder_id"))
        }
        _ => false,
    }
}

fn valid_u64_field(operation: &Value, field: &str, min: u64, max: u64) -> bool {
    operation
        .get(field)
        .and_then(Value::as_u64)
        .is_some_and(|value| (min..=max).contains(&value))
}

fn valid_optional_u64_field(operation: &Value, field: &str, min: u64, max: u64) -> bool {
    operation.get(field).is_none_or(|value| {
        value
            .as_u64()
            .is_some_and(|value| (min..=max).contains(&value))
    })
}

fn valid_axis_value(axis: &Value) -> bool {
    matches!(axis.as_str(), Some("x" | "y" | "z"))
}

fn valid_movement_value(movement: &Value) -> bool {
    let Some(object) = movement.as_object() else {
        return false;
    };
    object.len() == 2
        && movement.get("axis").is_some_and(valid_axis_value)
        && movement
            .get("delta_mm")
            .and_then(Value::as_f64)
            .is_some_and(|delta| delta.is_finite() && delta != 0.0 && delta.abs() <= 50.0)
}

fn valid_feedrate_value(feedrate: &Value) -> bool {
    feedrate
        .as_u64()
        .is_some_and(|feedrate| (1..=12_000).contains(&feedrate))
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

fn parse_single_command_operation(command: &str) -> Option<Value> {
    match command_code(command)? {
        "G28" => parse_home_operation(command),
        "M104" => parse_hotend_operation(command, false),
        "M109" => parse_hotend_operation(command, true),
        "M140" => parse_temperature_operation(command, "set_bed_temperature", false, false),
        "M190" => parse_temperature_operation(command, "set_bed_temperature", true, false),
        "M141" => parse_temperature_operation(command, "set_chamber_temperature", false, false),
        "M191" => parse_temperature_operation(command, "set_chamber_temperature", true, false),
        _ => None,
    }
}

fn parse_home_operation(command: &str) -> Option<Value> {
    let mut axes = Vec::new();
    for token in command.split_whitespace().skip(1) {
        let axis = match token.to_ascii_uppercase().as_str() {
            "X" => "x",
            "Y" => "y",
            "Z" => "z",
            _ => return None,
        };
        axes.push(axis);
    }
    Some(json!({ "action": "home", "axes": axes }))
}

fn parse_hotend_operation(command: &str, wait: bool) -> Option<Value> {
    parse_temperature_operation(command, "set_hotend_temperature", wait, true)
}

fn parse_temperature_operation(
    command: &str,
    action: &str,
    wait: bool,
    allow_extruder: bool,
) -> Option<Value> {
    let mut celsius = None;
    let mut extruder_id = None;
    for token in command.split_whitespace().skip(1) {
        let mut chars = token.chars();
        let parameter = chars.next()?.to_ascii_uppercase();
        let value = parse_gcode_number(chars.as_str())?;
        match parameter {
            'S' if celsius.is_none() => celsius = Some(value),
            'T' if allow_extruder && extruder_id.is_none() => extruder_id = Some(value),
            _ => return None,
        }
    }
    let celsius = parse_integer_gcode_value(celsius?)?;
    let mut body = serde_json::Map::from_iter([
        ("action".to_string(), json!(action)),
        ("temperature_celsius".to_string(), json!(celsius)),
        ("wait".to_string(), json!(wait)),
    ]);
    if let Some(value) = extruder_id {
        body.insert(
            "extruder_id".to_string(),
            json!(parse_integer_gcode_value(value)?),
        );
    }
    let operation = Value::Object(body);
    valid_operation_json(&operation).then_some(operation)
}

fn parse_move_axes_operation(command: &str) -> Option<Value> {
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
                if !movements.iter().any(|movement: &Value| {
                    movement["axis"] == parameter.to_ascii_lowercase().to_string()
                }) =>
            {
                movements.push(json!({
                    "axis": parameter.to_ascii_lowercase().to_string(),
                    "delta_mm": value,
                }));
            }
            'F' if feedrate.is_none() => feedrate = Some(value),
            _ => return None,
        }
    }
    if movements.is_empty() {
        return None;
    }

    let mut body = serde_json::Map::new();
    body.insert("action".to_string(), json!("move_axes"));
    body.insert("movements".to_string(), Value::Array(movements));
    if let Some(value) = feedrate {
        body.insert(
            "feedrate_mm_per_min".to_string(),
            json!(parse_integer_gcode_value(value)?),
        );
    }
    Some(Value::Object(body))
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

fn parse_integer_gcode_value(value: f64) -> Option<u32> {
    (value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f64).then_some(value as u32)
}
