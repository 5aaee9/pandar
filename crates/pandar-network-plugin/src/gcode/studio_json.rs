use serde_json::{Value, json};

use super::valid_operation_json;

pub(super) fn parse_studio_json_operation(value: &Value) -> Option<Value> {
    if let Some(system) = value.get("system") {
        return match system.get("command")?.as_str()? {
            "ledctrl" => parse_studio_ledctrl_operation(system),
            _ => None,
        };
    }

    parse_studio_print_operation(value.get("print")?)
}

fn parse_studio_ledctrl_operation(system: &Value) -> Option<Value> {
    let node = system.get("led_node")?.as_str()?;
    if !matches!(node, "chamber_light" | "chamber_light2") {
        return None;
    }

    let light_on = match system.get("led_mode")?.as_str()? {
        "on" => true,
        "off" => false,
        _ => return None,
    };
    Some(json!({ "action": "set_chamber_light", "light_on": light_on }))
}

fn parse_studio_print_operation(print: &Value) -> Option<Value> {
    match print.get("command")?.as_str()? {
        "pause" => Some(json!({ "action": "pause" })),
        "resume" => Some(json!({ "action": "resume" })),
        "stop" => Some(json!({ "action": "stop" })),
        "print_speed" => Some(json!({
            "action": "set_print_speed",
            "speed_mode": parse_json_u64(print.get("param")?)?,
        })),
        "select_extruder" => Some(json!({
            "action": "select_extruder",
            "extruder_id": parse_json_u64(print.get("extruder_index")?)?,
        })),
        "set_nozzle_temp" => Some(json!({
            "action": "set_hotend_temperature",
            "temperature_celsius": parse_json_u64(print.get("target_temp")?)?,
            "wait": false,
            "extruder_id": parse_json_u64(print.get("extruder_index")?)?,
        })),
        "set_bed_temp" => Some(json!({
            "action": "set_bed_temperature",
            "temperature_celsius": parse_json_u64(print.get("temp")?)?,
            "wait": false,
        })),
        "set_ctt" => Some(json!({
            "action": "set_chamber_temperature",
            "temperature_celsius": parse_json_u64(print.get("ctt_val")?)?,
            "wait": false,
        })),
        "ams_get_rfid" => Some(json!({
            "action": "ams_reread_rfid",
            "ams_id": parse_json_u64(print.get("ams_id")?)?,
            "slot_id": parse_json_u64(print.get("slot_id")?)?,
        })),
        "ams_change_filament" => parse_studio_ams_change_filament_operation(print),
        _ => None,
    }
    .filter(valid_operation_json)
}

fn parse_studio_ams_change_filament_operation(print: &Value) -> Option<Value> {
    let ams_id = parse_json_u64(print.get("ams_id")?)?;
    let slot_id = parse_json_u64(print.get("slot_id")?)?;
    let target = parse_json_u64(print.get("target")?)?;
    let mut body = if target == 255 && slot_id == 255 {
        serde_json::Map::from_iter([
            ("action".to_owned(), json!("ams_unload_filament")),
            ("ams_id".to_owned(), json!(ams_id)),
            ("slot_id".to_owned(), json!(slot_id)),
        ])
    } else {
        serde_json::Map::from_iter([
            ("action".to_owned(), json!("ams_load_filament")),
            ("ams_id".to_owned(), json!(ams_id)),
            ("slot_id".to_owned(), json!(slot_id)),
            ("global_tray_id".to_owned(), json!(target)),
        ])
    };
    if let Some(extruder_id) = print.get("extruder_id").and_then(parse_json_u64) {
        body.insert("extruder_id".to_owned(), json!(extruder_id));
    }
    Some(Value::Object(body))
}

fn parse_json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse::<u64>().ok())
}
