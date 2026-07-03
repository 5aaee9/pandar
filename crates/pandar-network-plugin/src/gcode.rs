use serde_json::{Value, json};

pub(super) fn operation_json_from_gcode(message: &str) -> Option<Value> {
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
    let Some(action) = operation.get("action").and_then(Value::as_str) else {
        return false;
    };

    match action {
        "pause" | "resume" | "stop" => operation
            .as_object()
            .is_some_and(|object| object.len() == 1),
        "set_print_speed" => {
            operation
                .get("speed_mode")
                .and_then(Value::as_u64)
                .is_some_and(|speed| (1..=4).contains(&speed))
                && operation
                    .as_object()
                    .is_some_and(|object| object.len() == 2)
        }
        "home" => {
            operation
                .get("axes")
                .and_then(Value::as_array)
                .is_some_and(|axes| axes.iter().all(valid_axis_value))
                && operation
                    .as_object()
                    .is_some_and(|object| object.len() == 2)
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
                && operation.as_object().is_some_and(|object| {
                    object.len()
                        == if operation.get("feedrate_mm_per_min").is_some() {
                            3
                        } else {
                            2
                        }
                })
        }
        "set_hotend_temperature" => {
            operation
                .get("temperature_celsius")
                .and_then(Value::as_u64)
                .is_some_and(|temperature| temperature <= 300)
                && operation.get("wait").is_none_or(Value::is_boolean)
                && operation.as_object().is_some_and(|object| {
                    object.len()
                        == if operation.get("wait").is_some() {
                            3
                        } else {
                            2
                        }
                })
        }
        _ => false,
    }
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
    let mut celsius = None;
    for token in command.split_whitespace().skip(1) {
        let mut chars = token.chars();
        let parameter = chars.next()?.to_ascii_uppercase();
        let value = parse_gcode_number(chars.as_str())?;
        match parameter {
            'S' if celsius.is_none() => celsius = Some(value),
            _ => return None,
        }
    }
    let celsius = parse_integer_gcode_value(celsius?)?;
    Some(json!({
        "action": "set_hotend_temperature",
        "temperature_celsius": celsius,
        "wait": wait,
    }))
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
