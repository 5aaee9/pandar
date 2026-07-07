use serde_json::Value;

pub fn printer_telemetry_fragment(printer_json: &str) -> String {
    let printer = serde_json::from_str::<Value>(printer_json).unwrap_or(Value::Null);
    let nozzles = array_items(&printer, "nozzle_temperatures");
    let nozzle = nozzles.first().copied().unwrap_or(&Value::Null);
    let right_nozzle = nozzles.get(1).copied().unwrap_or(&Value::Null);
    let nozzle_current = json_number_or_zero(field(nozzle, "current_celsius"));
    let nozzle_target = json_number_or_zero(field(nozzle, "target_celsius"));
    let right_nozzle_current = json_number_or_zero(field(right_nozzle, "current_celsius"));
    let right_nozzle_target = json_number_or_zero(field(right_nozzle, "target_celsius"));
    let bed_current = json_number_or_zero(field(&printer, "bed_temperature_celsius"));
    let bed_target = json_number_or_zero(field(&printer, "bed_target_temperature_celsius"));
    let chamber_current = json_number_or_zero(field(&printer, "chamber_temperature_celsius"));
    let active_nozzle = field(&printer, "active_nozzle");
    let light_mode = if printer
        .get("chamber_light_on")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "on"
    } else {
        "off"
    };
    let printer_type = field(&printer, "dev_model_name");

    format!(
        r#""printer_type":{},"support_chamber":true,"support_chamber_temp_display":true,"bed_temper":{},"bed_target_temper":{},"nozzle_type":"XS01","nozzle_diameter":0.4,"nozzle_temper":{},"nozzle_target_temper":{},"nozzle_temper2":{},"nozzle_target_temper2":{},"chamber_temper":{},"lights_report":[{{"node":"chamber_light","mode":{}}}],"device":{{"type":1,"bed_temp":{},"ctc":{{"state":1,"info":{{"temp":{}}}}},"nozzle":{},"extruder":{}}}{}"#,
        json_string(if printer_type.is_empty() {
            "C11"
        } else {
            &printer_type
        }),
        bed_current,
        bed_target,
        nozzle_current,
        nozzle_target,
        right_nozzle_current,
        right_nozzle_target,
        chamber_current,
        json_string(light_mode),
        packed_temperature_json(&bed_current, &bed_target),
        packed_temperature_json(&chamber_current, ""),
        studio_nozzle_device_json(&nozzles),
        studio_extruder_device_json(&nozzles, &active_nozzle),
        studio_materials_payload(&printer),
    )
}

fn field(value: &Value, key: &str) -> String {
    value.get(key).map(scalar_string).unwrap_or_default()
}

fn scalar_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

fn array_items<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("string JSON encoding cannot fail")
}

fn is_json_number(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    value.parse::<f64>().is_ok()
}

fn json_scalar_or_string(value: &str) -> String {
    if is_json_number(value) {
        value.to_string()
    } else {
        json_string(value)
    }
}

fn json_number_or_zero(value: String) -> String {
    if value.is_empty() {
        return "0".to_string();
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut out = String::new();
    for c in value.chars() {
        if c.is_ascii_digit() {
            seen_digit = true;
            out.push(c);
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            out.push(c);
        } else if (c == '-' || c == '+') && out.is_empty() {
            out.push(c);
        }
    }
    if !seen_digit || out == "-" || out == "+" {
        "0".to_string()
    } else {
        out
    }
}

fn json_temperature_bits(value: &str) -> u32 {
    let parsed = json_number_or_zero(value.to_string())
        .parse::<f64>()
        .unwrap_or(0.0);
    if parsed <= 0.0 {
        0
    } else if parsed >= 65535.0 {
        65535
    } else {
        (parsed + 0.5) as u32
    }
}

fn packed_temperature_json(current: &str, target: &str) -> String {
    (json_temperature_bits(current) | (json_temperature_bits(target) << 16)).to_string()
}

fn studio_extruder_id(label: &str, index: usize, total: usize) -> u32 {
    if total <= 1 {
        return 0;
    }
    if label.eq_ignore_ascii_case("L") {
        return 1;
    }
    if label.eq_ignore_ascii_case("R") {
        return 0;
    }
    if index == 0 { 1 } else { 0 }
}

fn studio_active_extruder_id(nozzles: &[&Value], active_nozzle: &str) -> u32 {
    if nozzles.len() <= 1 {
        return 0;
    }
    if active_nozzle.eq_ignore_ascii_case("L") {
        return 1;
    }
    if active_nozzle.eq_ignore_ascii_case("R") {
        return 0;
    }
    studio_extruder_id(&field(nozzles[0], "label"), 0, nozzles.len())
}

fn studio_extruder_device_json(nozzles: &[&Value], active_nozzle: &str) -> String {
    let total = nozzles.len().max(1);
    let active_id = studio_active_extruder_id(nozzles, active_nozzle);
    let mut info = String::from("[");
    for i in 0..total {
        let nozzle = nozzles.get(i).copied().unwrap_or(&Value::Null);
        let id = studio_extruder_id(&field(nozzle, "label"), i, total);
        let temp = packed_temperature_json(
            &field(nozzle, "current_celsius"),
            &field(nozzle, "target_celsius"),
        );
        if i != 0 {
            info.push(',');
        }
        info.push_str(&format!(
            r#"{{"id":{id},"info":8,"temp":{temp},"spre":65535,"snow":65535,"star":65535,"stat":0,"hnow":{id}}}"#
        ));
    }
    info.push(']');
    format!(
        r#"{{"state":{},"info":{info}}}"#,
        total | ((active_id as usize) << 4)
    )
}

fn studio_nozzle_device_json(nozzles: &[&Value]) -> String {
    let total = nozzles.len().max(1);
    let mut exist = 0;
    let mut info = String::from("[");
    for i in 0..total {
        let nozzle = nozzles.get(i).copied().unwrap_or(&Value::Null);
        let id = studio_extruder_id(&field(nozzle, "label"), i, total);
        exist |= 1_u32 << id;
        if i != 0 {
            info.push(',');
        }
        info.push_str(&format!(
            r#"{{"id":{id},"diameter":0.4,"type":"XS01","stat":0}}"#
        ));
    }
    info.push(']');
    format!(r#"{{"exist":{exist},"state":0,"info":{info}}}"#)
}

fn parse_u64_or_zero(value: &str) -> u64 {
    value.parse().unwrap_or(0)
}

fn hex_string(value: u64) -> String {
    format!("{value:x}")
}

fn studio_tray_json(tray: &Value) -> Option<String> {
    let tray_id = field(tray, "tray_id");
    if tray_id.is_empty() {
        return None;
    }
    let mut out = format!(r#"{{"id":{}"#, json_string(&tray_id));
    append_string_field(&mut out, tray, "filament_id", "tray_info_idx");
    append_string_field(&mut out, tray, "type", "tray_type");
    append_string_field(&mut out, tray, "color", "tray_color");
    append_scalar_field(&mut out, tray, "k_value", "k");
    append_scalar_field(&mut out, tray, "remaining_estimate", "remain");
    out.push('}');
    Some(out)
}

fn append_string_field(out: &mut String, value: &Value, source: &str, target: &str) {
    let value = field(value, source);
    if !value.is_empty() {
        out.push_str(&format!(r#","{target}":{}"#, json_string(&value)));
    }
}

fn append_scalar_field(out: &mut String, value: &Value, source: &str, target: &str) {
    let value = field(value, source);
    if !value.is_empty() {
        out.push_str(&format!(r#","{target}":{}"#, json_scalar_or_string(&value)));
    }
}

fn studio_ams_unit_json(
    unit: &Value,
    ams_exist_bits: &mut u64,
    tray_exist_bits: &mut u64,
) -> Option<String> {
    let unit_id = field(unit, "unit_id");
    if unit_id.is_empty() {
        return None;
    }
    let unit_number = parse_u64_or_zero(&unit_id);
    if unit_number < 64 {
        *ams_exist_bits |= 1_u64 << unit_number;
    }

    let toolhead = field(unit, "toolhead");
    let extruder_id = if toolhead.eq_ignore_ascii_case("L") {
        1
    } else {
        0
    };
    let info = hex_string(1 | (extruder_id << 8));
    let mut out = format!(
        r#"{{"id":{},"info":{}"#,
        json_string(&unit_id),
        json_string(&info)
    );
    append_string_field(&mut out, unit, "humidity_level", "humidity");
    append_string_field(&mut out, unit, "humidity", "humidity_raw");
    append_string_field(&mut out, unit, "temperature_celsius", "temp");
    out.push_str(r#","tray":["#);

    let mut first = true;
    for tray in array_items(unit, "trays") {
        let Some(tray_json) = studio_tray_json(tray) else {
            continue;
        };
        if !first {
            out.push(',');
        }
        out.push_str(&tray_json);
        first = false;

        let global = field(tray, "global_tray_id");
        let global_number = if global.is_empty() {
            unit_number * 4 + parse_u64_or_zero(&field(tray, "tray_id"))
        } else {
            parse_u64_or_zero(&global)
        };
        if global_number < 64 {
            *tray_exist_bits |= 1_u64 << global_number;
        }
    }
    out.push_str("]}");
    Some(out)
}

fn studio_virtual_slot_json(spool: &Value, index: usize) -> String {
    let toolhead = field(spool, "toolhead");
    let mut id = if toolhead.eq_ignore_ascii_case("L") {
        "254".to_string()
    } else if toolhead.eq_ignore_ascii_case("R") {
        "255".to_string()
    } else {
        field(spool, "external_id")
    };
    if id != "254" && id != "255" {
        id = if index == 0 { "255" } else { "254" }.to_string();
    }

    let mut out = format!(r#"{{"id":{}"#, json_string(&id));
    append_string_field(&mut out, spool, "filament_id", "tray_info_idx");
    append_string_field(&mut out, spool, "type", "tray_type");
    append_string_field(&mut out, spool, "color", "tray_color");
    append_scalar_field(&mut out, spool, "k_value", "k");
    append_scalar_field(&mut out, spool, "remaining_estimate", "remain");
    out.push('}');
    out
}

fn studio_tray_now_json(materials: &Value) -> String {
    let Some(active) = materials.get("active_tray") else {
        return String::new();
    };
    let global = field(active, "global_tray_id");
    if !global.is_empty() {
        return format!(r#","tray_now":{}"#, json_string(&global));
    }
    if field(active, "kind") == "external" {
        let external_id = field(active, "external_id");
        return format!(
            r#","tray_now":{}"#,
            json_string(if external_id.is_empty() {
                "255"
            } else {
                &external_id
            })
        );
    }
    let ams_id = parse_u64_or_zero(&field(active, "ams_id"));
    let tray_id = parse_u64_or_zero(&field(active, "tray_id"));
    format!(
        r#","tray_now":{}"#,
        json_string(&(ams_id * 4 + tray_id).to_string())
    )
}

fn studio_materials_payload(printer: &Value) -> String {
    let Some(materials) = printer.get("materials") else {
        return r#","ams":{"ams":[]}"#.to_string();
    };

    let mut ams_exist_bits = 0;
    let mut tray_exist_bits = 0;
    let mut ams_units = String::new();
    let mut first = true;
    for unit in array_items(materials, "ams_units") {
        let Some(unit_json) = studio_ams_unit_json(unit, &mut ams_exist_bits, &mut tray_exist_bits)
        else {
            continue;
        };
        if !first {
            ams_units.push(',');
        }
        ams_units.push_str(&unit_json);
        first = false;
    }

    let mut out = format!(
        r#","ams":{{"ams":[{ams_units}],"ams_exist_bits":{},"tray_exist_bits":{}{}"#,
        json_string(&hex_string(ams_exist_bits)),
        json_string(&hex_string(tray_exist_bits)),
        studio_tray_now_json(materials),
    );
    out.push('}');

    let external_spools = array_items(materials, "external_spools");
    if !external_spools.is_empty() {
        out.push_str(r#","vir_slot":["#);
        for (index, spool) in external_spools.into_iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            out.push_str(&studio_virtual_slot_json(spool, index));
        }
        out.push(']');
    }
    out
}
