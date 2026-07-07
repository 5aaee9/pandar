mod external;
mod identifiers;
mod patch;
mod schema;

use external::{has_dual_external_slots, normalize_external_spools};
use identifiers::*;
use patch::*;
use schema::*;
use serde_json::{Map, Value};

struct NormalizedTrayPatch {
    tray_id: String,
    value: Value,
}

pub fn normalize_material_patch(report: &Value, observed_at: &str) -> Option<Value> {
    let report = serde_json::from_value::<MaterialsReport>(report.clone()).ok()?;
    let print = report.print?;
    let ams = print.ams.as_ref()?;
    let mut patch = MaterialPatchDocument {
        document_type: "printer_material_patch",
        observed_at,
        ams_units: Vec::new(),
        external_spools: None,
        replace_external_spools: false,
        active_tray: None,
    };

    if !ams.ams.is_empty() {
        let normalized_units = normalize_ams_units(&ams.ams, ams, &print);
        if !normalized_units.is_empty() {
            patch.ams_units = normalized_units;
        }
    }

    if let Some(external) = normalize_external_spools(&print) {
        patch.external_spools = Some(external.spools);
        patch.replace_external_spools = external.replace;
    }

    if let Some(active_tray) = normalize_active_tray(ams.tray_now.as_ref()) {
        patch.active_tray = Some(active_tray);
    }

    (!patch.ams_units.is_empty()
        || patch.external_spools.is_some()
        || patch.replace_external_spools
        || patch.active_tray.is_some())
    .then(|| serde_json::to_value(patch).expect("material patch is serializable"))
}

fn normalize_ams_units(
    units: &[AmsUnitReport],
    ams: &AmsReport,
    print: &PrintMaterialsReport,
) -> Vec<Value> {
    let power_on = ams.power_on_flag;
    let tray_exist_bits = parse_tray_exist_bits(ams.tray_exist_bits.as_ref());
    let skip_zero_poweroff_cleanup = power_on == Some(false) && tray_exist_bits == Some(0);
    let dual_nozzle = has_dual_nozzle_report(print, ams);

    units
        .iter()
        .filter_map(|unit| {
            let unit_id = unit_id(unit)?;
            let unit_kind = unit_kind(&unit_id);
            let mut normalized = Map::new();
            normalized.insert("unit_id".to_owned(), Value::String(unit_id.clone()));
            normalized.insert("unit_kind".to_owned(), Value::String(unit_kind.to_owned()));
            insert_number_field(&mut normalized, "humidity", unit.humidity_raw.as_ref());
            insert_number_field(&mut normalized, "humidity_level", unit.humidity.as_ref());
            insert_number_field(
                &mut normalized,
                "temperature_celsius",
                unit.temperature_celsius.as_ref().or(unit.temp.as_ref()),
            );
            if let Some(toolhead) = unit
                .info
                .as_ref()
                .and_then(normalize_toolhead)
                .or_else(|| default_dual_ams_toolhead(unit, units.len(), dual_nozzle))
            {
                normalized.insert("toolhead".to_owned(), Value::String(toolhead));
            }

            if !unit.trays.is_empty() {
                let mut normalized_trays: Vec<NormalizedTrayPatch> = unit
                    .trays
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
                        normalized_trays.iter().any(|tray| tray.tray_id == tray_id)
                    });
                normalized.insert(
                    "trays".to_owned(),
                    Value::Array(
                        normalized_trays
                            .into_iter()
                            .map(|tray| tray.value)
                            .collect(),
                    ),
                );
                if replace_trays {
                    normalized.insert("replace_trays".to_owned(), Value::Bool(true));
                }
            }

            (normalized.len() > 2).then_some(Value::Object(normalized))
        })
        .collect()
}

fn normalize_tray(
    tray: &MaterialSlotReport,
    unit_id: &str,
    unit_kind: &str,
) -> Option<NormalizedTrayPatch> {
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
    Some(NormalizedTrayPatch {
        tray_id,
        value: Value::Object(normalized),
    })
}

fn apply_material_fields(normalized: &mut Map<String, Value>, source: &MaterialSlotReport) {
    insert_string_field(normalized, "state", source.state.as_ref());
    let filament_id = normalized_string(source.tray_info_idx.as_ref()).or_else(|| {
        normalized_string(source.setting_id.as_ref())
            .map(|setting_id| derive_filament_id(&setting_id))
    });
    let setting_id = normalized_string(source.setting_id.as_ref()).or_else(|| {
        normalized_string(source.tray_info_idx.as_ref())
            .map(|filament_id| derive_setting_id(&filament_id))
    });
    if let Some(filament_id) = filament_id {
        normalized.insert("filament_id".to_owned(), Value::String(filament_id));
    }
    if let Some(setting_id) = setting_id {
        normalized.insert("setting_id".to_owned(), Value::String(setting_id));
    }
    insert_string_field(normalized, "type", source.tray_type.as_ref());
    insert_string_field(normalized, "tag_uid", source.tag_uid.as_ref());
    insert_string_field(normalized, "tray_uuid", source.tray_uuid.as_ref());
    insert_string_field(normalized, "name", source.tray_sub_brands.as_ref());
    insert_string_field(normalized, "remaining_estimate", source.remain.as_ref());
    insert_string_field(
        normalized,
        "k_value",
        source.k.as_ref().or(source.k_value.as_ref()),
    );

    if let Some(color) = source.tray_color.as_ref().and_then(normalize_color) {
        normalized.insert("color".to_owned(), Value::String(color));
    }
    if let Some(multi_color) = source.cols.as_ref().and_then(normalize_multi_color) {
        normalized.insert("multi_color".to_owned(), Value::Array(multi_color));
    }
    if let Some(toolhead) = source
        .toolhead
        .as_ref()
        .and_then(|value| normalized_string(Some(value)))
        .or_else(|| {
            source
                .extruder_id
                .as_ref()
                .and_then(normalize_extruder_toolhead)
        })
    {
        normalized.insert("toolhead".to_owned(), Value::String(toolhead));
    }
}

fn insert_string_field(
    normalized: &mut Map<String, Value>,
    target: &str,
    value: Option<&ScalarValue>,
) {
    if let Some(value) = normalized_string(value) {
        normalized.insert(target.to_owned(), Value::String(value));
    }
}

fn insert_number_field(
    normalized: &mut Map<String, Value>,
    target: &str,
    value: Option<&ScalarValue>,
) {
    if let Some(value) = value.and_then(normalized_number) {
        normalized.insert(target.to_owned(), value);
    }
}

fn normalized_number(value: &ScalarValue) -> Option<Value> {
    value.number_json()
}

pub(in crate::machine::materials) fn normalized_string(
    value: Option<&ScalarValue>,
) -> Option<String> {
    value?.string()
}

fn apply_empty_tray_clears(trays: &mut Vec<NormalizedTrayPatch>, unit_id: &str, bits: u64) {
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
        if let Some(existing) = trays.iter().position(|tray| tray.tray_id == tray_id) {
            trays[existing] = clear;
            continue;
        }

        trays.push(clear);
    }
}

fn empty_tray_clear(unit_id: &str, slot: u64) -> NormalizedTrayPatch {
    let tray_id = slot.to_string();
    let value = empty_tray_clear_value(tray_id, global_tray_id(unit_id, &slot.to_string()));
    NormalizedTrayPatch {
        tray_id: slot.to_string(),
        value,
    }
}

fn has_dual_nozzle_report(print: &PrintMaterialsReport, ams: &AmsReport) -> bool {
    print.nozzle_temper2.is_some()
        || print.right_nozzle_temper.is_some()
        || print
            .nozzles
            .as_ref()
            .is_some_and(|nozzles| nozzles.len() > 1)
        || has_dual_external_slots(print, ams)
}

fn default_dual_ams_toolhead(
    unit: &AmsUnitReport,
    unit_count: usize,
    dual_nozzle: bool,
) -> Option<String> {
    if !dual_nozzle || unit_count != 2 {
        return None;
    }
    match unit_id(unit)?.as_str() {
        "0" => Some("R".to_owned()),
        "1" => Some("L".to_owned()),
        _ => None,
    }
}

fn normalize_active_tray(value: Option<&ScalarValue>) -> Option<Value> {
    let tray_now = parse_i64(value?)?;
    match tray_now {
        255 => Some(Value::Null),
        254 => Some(external_active_tray_value()),
        0..=15 => Some(ams_active_tray_value(tray_now)),
        128..=135 => Some(ams_ht_active_tray_value(tray_now)),
        _ => None,
    }
}

fn normalize_color(value: &ScalarValue) -> Option<String> {
    let raw = value.string()?;
    normalize_color_str(&raw)
}

fn normalize_color_str(value: &str) -> Option<String> {
    let raw = value.trim().trim_start_matches('#');
    let valid_len = raw.len() == 6 || raw.len() == 8;
    let valid_hex = raw.chars().all(|ch| ch.is_ascii_hexdigit());
    (valid_len && valid_hex).then(|| raw.to_ascii_uppercase())
}

fn normalize_multi_color(value: &ColorSource) -> Option<Vec<Value>> {
    let colors: Vec<Value> = match value {
        ColorSource::List(values) => values
            .iter()
            .filter_map(normalize_color)
            .map(Value::String)
            .collect(),
        ColorSource::Single(ScalarValue::String(raw)) => raw
            .split(',')
            .filter_map(normalize_color_str)
            .map(Value::String)
            .collect(),
        ColorSource::Single(value) => normalize_color(value)
            .map(Value::String)
            .into_iter()
            .collect(),
    };
    (!colors.is_empty()).then_some(colors)
}

fn normalize_toolhead(value: &ScalarValue) -> Option<String> {
    let raw = normalized_string(Some(value))?;
    let parsed =
        u64::from_str_radix(raw.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
    match (parsed >> 8) & 0xF {
        0 => Some("R".to_owned()),
        1 => Some("L".to_owned()),
        _ => None,
    }
}

pub(in crate::machine::materials) fn normalize_extruder_toolhead(
    value: &ScalarValue,
) -> Option<String> {
    match parse_i64(value)? {
        0 => Some("R".to_owned()),
        1 => Some("L".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
