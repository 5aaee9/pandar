use serde_json::{Map, Value, json};

pub fn normalize_material_patch(report: &Value, observed_at: &str) -> Option<Value> {
    let print = report.get("print")?;
    let ams = print.get("ams")?;
    let mut patch = Map::new();
    patch.insert("type".to_owned(), json!("printer_material_patch"));
    patch.insert("observed_at".to_owned(), json!(observed_at));

    if let Some(units) = ams.get("ams").and_then(Value::as_array) {
        let normalized_units = normalize_ams_units(units, ams);
        if !normalized_units.is_empty() {
            patch.insert("ams_units".to_owned(), Value::Array(normalized_units));
        }
    }

    if let Some(external) = normalize_external_spools(ams) {
        patch.insert("external_spools".to_owned(), Value::Array(external.spools));
        if external.replace {
            patch.insert("replace_external_spools".to_owned(), Value::Bool(true));
        }
    }

    if let Some(active_tray) = normalize_active_tray(ams.get("tray_now")) {
        patch.insert("active_tray".to_owned(), active_tray);
    }

    (patch.len() > 2).then_some(Value::Object(patch))
}

struct ExternalSpoolsPatch {
    spools: Vec<Value>,
    replace: bool,
}

fn normalize_ams_units(units: &[Value], ams: &Value) -> Vec<Value> {
    let power_on = ams.get("power_on_flag").and_then(Value::as_bool);
    let tray_exist_bits = parse_tray_exist_bits(ams.get("tray_exist_bits"));
    let skip_zero_poweroff_cleanup = power_on == Some(false) && tray_exist_bits == Some(0);

    units
        .iter()
        .filter_map(|unit| {
            let unit_id = unit_id(unit)?;
            let unit_kind = unit_kind(&unit_id);
            let mut normalized = Map::new();
            normalized.insert("unit_id".to_owned(), Value::String(unit_id.clone()));
            normalized.insert("unit_kind".to_owned(), Value::String(unit_kind.to_owned()));
            insert_number_field(&mut normalized, "humidity", unit.get("humidity"));
            insert_number_field(
                &mut normalized,
                "temperature_celsius",
                unit.get("temperature_celsius").or_else(|| unit.get("temp")),
            );

            if let Some(trays) = unit.get("tray").and_then(Value::as_array) {
                let mut normalized_trays: Vec<Value> = trays
                    .iter()
                    .filter_map(|tray| normalize_tray(tray, &unit_id, unit_kind))
                    .collect();
                if unit_kind == "ams"
                    && !skip_zero_poweroff_cleanup
                    && let Some(bits) = tray_exist_bits
                {
                    apply_empty_tray_clears(&mut normalized_trays, &unit_id, bits);
                }
                let replace_trays = unit_kind != "ams"
                    || (0..4).all(|slot| {
                        let tray_id = slot.to_string();
                        normalized_trays.iter().any(|tray| {
                            tray.get("tray_id").and_then(Value::as_str) == Some(tray_id.as_str())
                        })
                    });
                normalized.insert("trays".to_owned(), Value::Array(normalized_trays));
                if replace_trays {
                    normalized.insert("replace_trays".to_owned(), Value::Bool(true));
                }
            }

            (normalized.len() > 2).then_some(Value::Object(normalized))
        })
        .collect()
}

fn normalize_tray(tray: &Value, unit_id: &str, unit_kind: &str) -> Option<Value> {
    let tray_id = tray_id(tray)?;
    let mut normalized = Map::new();
    normalized.insert("tray_id".to_owned(), Value::String(tray_id.clone()));
    normalized.insert("exists".to_owned(), Value::Bool(true));
    normalized.insert("unit_kind".to_owned(), Value::String(unit_kind.to_owned()));
    normalized.insert(
        "global_tray_id".to_owned(),
        global_tray_id(unit_id, &tray_id).map_or(Value::Null, Value::from),
    );

    apply_material_fields(&mut normalized, tray);
    Some(Value::Object(normalized))
}

fn apply_material_fields(normalized: &mut Map<String, Value>, source: &Value) {
    insert_string_field(normalized, "state", source.get("state"));
    insert_string_field(normalized, "filament_id", source.get("tray_info_idx"));
    insert_string_field(normalized, "setting_id", source.get("setting_id"));
    insert_string_field(normalized, "type", source.get("tray_type"));
    insert_string_field(normalized, "tag_uid", source.get("tag_uid"));
    insert_string_field(normalized, "tray_uuid", source.get("tray_uuid"));
    insert_string_field(normalized, "name", source.get("tray_sub_brands"));
    insert_string_field(normalized, "remaining_estimate", source.get("remain"));

    if let Some(color) = source.get("tray_color").and_then(normalize_color) {
        normalized.insert("color".to_owned(), Value::String(color));
    }
    if let Some(multi_color) = source.get("cols").and_then(normalize_multi_color) {
        normalized.insert("multi_color".to_owned(), Value::Array(multi_color));
    }

    if !normalized.contains_key("setting_id")
        && let Some(filament_id) = normalized.get("filament_id").and_then(Value::as_str)
    {
        normalized.insert(
            "setting_id".to_owned(),
            Value::String(derive_setting_id(filament_id)),
        );
    }
    if !normalized.contains_key("filament_id")
        && let Some(setting_id) = normalized.get("setting_id").and_then(Value::as_str)
    {
        normalized.insert(
            "filament_id".to_owned(),
            Value::String(derive_filament_id(setting_id)),
        );
    }
}

fn insert_string_field(normalized: &mut Map<String, Value>, target: &str, value: Option<&Value>) {
    if let Some(value) = normalized_string(value) {
        normalized.insert(target.to_owned(), Value::String(value));
    }
}

fn insert_number_field(normalized: &mut Map<String, Value>, target: &str, value: Option<&Value>) {
    if let Some(value) = value.and_then(normalized_number) {
        normalized.insert(target.to_owned(), value);
    }
}

fn normalized_number(value: &Value) -> Option<Value> {
    match value {
        Value::Number(_) => Some(value.clone()),
        Value::String(raw) => raw.trim().parse::<i64>().ok().map(Value::from),
        _ => None,
    }
}

fn normalized_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(raw) => {
            let trimmed = raw.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Value::Number(_) | Value::Bool(_) => Some(value?.to_string()),
        _ => None,
    }
}

fn apply_empty_tray_clears(trays: &mut Vec<Value>, unit_id: &str, bits: u64) {
    let Some(unit_offset) = unit_id.parse::<u64>().ok().map(|unit| unit * 4) else {
        return;
    };
    for slot in 0..4 {
        let bit_index = unit_offset + slot;
        if bit_index < u64::BITS as u64 && bits & (1_u64 << bit_index) != 0 {
            continue;
        }
        let tray_id = slot.to_string();
        let clear = empty_tray_clear(unit_id, slot);
        if let Some(existing) = trays
            .iter()
            .position(|tray| tray.get("tray_id").and_then(Value::as_str) == Some(tray_id.as_str()))
        {
            trays[existing] = clear;
            continue;
        }

        trays.push(clear);
    }
}

fn empty_tray_clear(unit_id: &str, slot: u64) -> Value {
    let tray_id = slot.to_string();
    json!({
        "tray_id": tray_id,
        "exists": false,
        "unit_kind": "ams",
        "global_tray_id": global_tray_id(unit_id, &slot.to_string()),
        "state": "9",
        "filament_id": null,
        "setting_id": null,
        "type": null,
        "color": null,
        "multi_color": null,
        "tag_uid": null,
        "tray_uuid": null,
        "name": null,
        "remaining_estimate": null
    })
}

fn normalize_external_spools(ams: &Value) -> Option<ExternalSpoolsPatch> {
    if let Some(vir_slot) = ams.get("vir_slot") {
        return normalize_external_source(vir_slot, true);
    }
    ams.get("vt_tray")
        .and_then(|vt_tray| normalize_external_source(vt_tray, false))
}

fn normalize_external_source(value: &Value, vir_slot: bool) -> Option<ExternalSpoolsPatch> {
    let (entries, replace_single) = match value {
        Value::Array(entries) => (entries.iter().collect::<Vec<_>>(), vir_slot),
        Value::Object(_) => (vec![value], false),
        _ => return None,
    };
    if entries.is_empty() {
        return Some(ExternalSpoolsPatch {
            spools: Vec::new(),
            replace: true,
        });
    }

    let multi = entries.len() > 1;
    let spools = entries
        .iter()
        .enumerate()
        .map(|(index, spool)| normalize_external_spool(spool, index, multi))
        .collect();

    Some(ExternalSpoolsPatch {
        spools,
        replace: replace_single || multi,
    })
}

fn normalize_external_spool(spool: &Value, index: usize, multi: bool) -> Value {
    let mut normalized = Map::new();
    normalized.insert("external_id".to_owned(), Value::String("254".to_owned()));
    normalized.insert("exists".to_owned(), Value::Bool(true));
    normalized.insert(
        "tray_id".to_owned(),
        Value::String(if multi {
            index.to_string()
        } else {
            "0".to_owned()
        }),
    );
    apply_material_fields(&mut normalized, spool);
    Value::Object(normalized)
}

fn normalize_active_tray(value: Option<&Value>) -> Option<Value> {
    let tray_now = parse_i64(value?)?;
    match tray_now {
        255 => Some(Value::Null),
        254 => Some(json!({
            "kind": "external",
            "external_id": "254",
            "tray_id": "0",
            "global_tray_id": null
        })),
        0..=15 => Some(json!({
            "kind": "ams",
            "global_tray_id": tray_now,
            "ams_id": (tray_now / 4).to_string(),
            "tray_id": (tray_now % 4).to_string()
        })),
        128..=135 => Some(json!({
            "kind": "ams_ht",
            "global_tray_id": null,
            "ams_id": tray_now.to_string(),
            "tray_id": "0"
        })),
        _ => None,
    }
}

fn normalize_color(value: &Value) -> Option<String> {
    let raw = value.as_str()?.trim().trim_start_matches('#');
    let valid_len = raw.len() == 6 || raw.len() == 8;
    let valid_hex = raw.chars().all(|ch| ch.is_ascii_hexdigit());
    (valid_len && valid_hex).then(|| raw.to_ascii_uppercase())
}

fn normalize_multi_color(value: &Value) -> Option<Vec<Value>> {
    let colors: Vec<Value> = match value {
        Value::Array(values) => values
            .iter()
            .filter_map(normalize_color)
            .map(Value::String)
            .collect(),
        Value::String(raw) => raw
            .split(',')
            .filter_map(|part| normalize_color(&Value::String(part.to_owned())))
            .map(Value::String)
            .collect(),
        _ => return None,
    };
    (!colors.is_empty()).then_some(colors)
}

fn derive_setting_id(filament_id: &str) -> String {
    let base = strip_version_suffix(filament_id);
    if let Some(rest) = base.strip_prefix("GFL") {
        return format!("GFSL{rest}");
    }
    base.to_owned()
}

fn derive_filament_id(setting_id: &str) -> String {
    let base = strip_version_suffix(setting_id);
    if let Some(rest) = base.strip_prefix("GFSL") {
        return format!("GFL{rest}");
    }
    base.to_owned()
}

fn strip_version_suffix(value: &str) -> &str {
    let Some((base, suffix)) = value.rsplit_once('_') else {
        return value;
    };
    if suffix.chars().all(|ch| ch.is_ascii_digit()) {
        base
    } else {
        value
    }
}

fn unit_id(unit: &Value) -> Option<String> {
    normalized_string(unit.get("id")).or_else(|| normalized_string(unit.get("ams_id")))
}

fn tray_id(tray: &Value) -> Option<String> {
    normalized_string(tray.get("id")).or_else(|| normalized_string(tray.get("tray_id")))
}

fn unit_kind(unit_id: &str) -> &'static str {
    match unit_id.parse::<u32>() {
        Ok(0..=63) => "ams",
        Ok(128..=135) => "ams_ht",
        _ => "unknown",
    }
}

fn global_tray_id(unit_id: &str, tray_id: &str) -> Option<u64> {
    let unit_id = unit_id.parse::<u64>().ok()?;
    let tray_id = tray_id.parse::<u64>().ok()?;
    (unit_id < 64).then_some(unit_id * 4 + tray_id)
}

fn parse_tray_exist_bits(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(raw) => {
            let trimmed = raw.trim();
            let hex = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"));
            match hex {
                Some(hex) => u64::from_str_radix(hex, 16).ok(),
                None => trimmed.parse::<u64>().ok(),
            }
        }
        _ => None,
    }
}

fn parse_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| number.as_u64()?.try_into().ok()),
        Value::String(raw) => raw.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
