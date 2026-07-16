mod external;
mod identifiers;
mod patch;
mod schema;

use external::{has_dual_external_slots, normalize_external_spools};
use identifiers::*;
use patch::*;
use schema::*;
use serde_json::{Number, Value};

use super::types::decode_json_payload;

struct NormalizedTrayPatch {
    tray_id: String,
    value: MaterialTrayPatch,
}

pub(crate) fn parse_materials_report(report: &Value) -> Option<MaterialsReport> {
    decode_json_payload(report)
}

pub(crate) fn normalize_material_patch<'a>(
    report: &MaterialsReport,
    observed_at: &'a str,
) -> Option<MaterialPatchDocument<'a>> {
    let print = report.print.as_ref()?;
    let filament_switch_installed = filament_switch_installed(print.aux.as_ref());
    let mut patch = MaterialPatchDocument {
        document_type: "printer_material_patch",
        observed_at,
        filament_switch_installed,
        ams_units: Vec::new(),
        external_spools: None,
        replace_external_spools: false,
        active_tray: None,
    };

    if let Some(ams) = print.ams.as_ref() {
        if !ams.ams.is_empty() {
            let normalized_units =
                normalize_ams_units(&ams.ams, ams, print, filament_switch_installed);
            if !normalized_units.is_empty() {
                patch.ams_units = normalized_units;
            }
        }

        if let Some(active_tray) = normalize_active_tray(ams.tray_now.as_ref()) {
            patch.active_tray = Some(active_tray);
        }
    }

    if let Some(external) = normalize_external_spools(print) {
        patch.external_spools = Some(external.spools);
        patch.replace_external_spools = external.replace;
    }

    (patch.filament_switch_installed.is_some()
        || !patch.ams_units.is_empty()
        || patch.external_spools.is_some()
        || patch.replace_external_spools
        || patch.active_tray.is_some())
    .then_some(patch)
}

fn normalize_ams_units(
    units: &[AmsUnitReport],
    ams: &AmsReport,
    print: &PrintMaterialsReport,
    filament_switch_installed: Option<bool>,
) -> Vec<AmsUnitPatch> {
    let power_on = ams.power_on_flag;
    let tray_exist_bits = parse_tray_exist_bits(ams.tray_exist_bits.as_ref());
    let skip_zero_poweroff_cleanup = power_on == Some(false) && tray_exist_bits == Some(0);
    let dual_nozzle = has_dual_nozzle_report(print, ams);

    units
        .iter()
        .filter_map(|unit| {
            let unit_id = unit_id(unit)?;
            let unit_kind = unit_kind(&unit_id);
            let info = unit.info.as_ref().and_then(normalize_ams_info);
            let toolhead = match unit.info.as_ref() {
                Some(info) => normalize_toolhead(info, filament_switch_installed),
                None if filament_switch_installed == Some(true) => None,
                None => default_dual_ams_toolhead(unit, units.len(), dual_nozzle),
            };
            let mut trays = Vec::new();
            let mut replace_trays = false;

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
                replace_trays = unit_kind != "ams"
                    || (0..4).all(|slot| {
                        let tray_id = slot.to_string();
                        normalized_trays.iter().any(|tray| tray.tray_id == tray_id)
                    });
                trays = normalized_trays
                    .into_iter()
                    .map(|tray| tray.value)
                    .collect();
            }

            (unit.humidity_raw.is_some()
                || unit.humidity.is_some()
                || unit.temperature_celsius.is_some()
                || unit.temp.is_some()
                || info.is_some()
                || toolhead.is_some()
                || !trays.is_empty()
                || replace_trays)
                .then_some(AmsUnitPatch {
                    unit_id,
                    unit_kind: unit_kind.to_owned(),
                    info,
                    humidity: unit.humidity_raw.as_ref().and_then(normalized_number),
                    humidity_level: unit.humidity.as_ref().and_then(normalized_number),
                    temperature_celsius: unit
                        .temperature_celsius
                        .as_ref()
                        .or(unit.temp.as_ref())
                        .and_then(normalized_number),
                    toolhead,
                    trays,
                    replace_trays,
                })
        })
        .collect()
}

fn normalize_tray(
    tray: &MaterialSlotReport,
    unit_id: &str,
    unit_kind: &str,
) -> Option<NormalizedTrayPatch> {
    let tray_id = tray_id(tray)?;
    let global_tray_id = global_tray_id(unit_id, &tray_id);
    Some(NormalizedTrayPatch {
        tray_id: tray_id.clone(),
        value: MaterialTrayPatch::Present(MaterialTrayEntryPatch {
            tray_id,
            exists: true,
            unit_kind: unit_kind.to_owned(),
            global_tray_id,
            fields: material_fields(tray),
        }),
    })
}

pub(in crate::machine::materials) fn material_fields(
    source: &MaterialSlotReport,
) -> MaterialFieldsPatch {
    let filament_id = normalized_string(source.tray_info_idx.as_ref()).or_else(|| {
        normalized_string(source.setting_id.as_ref())
            .map(|setting_id| derive_filament_id(&setting_id))
    });
    let setting_id = normalized_string(source.setting_id.as_ref()).or_else(|| {
        normalized_string(source.tray_info_idx.as_ref())
            .map(|filament_id| derive_setting_id(&filament_id))
    });
    MaterialFieldsPatch {
        state: normalized_string(source.state.as_ref()),
        filament_id,
        setting_id,
        filament_type: normalized_string(source.tray_type.as_ref()),
        color: source.tray_color.as_ref().and_then(normalize_color),
        multi_color: source.cols.as_ref().and_then(normalize_multi_color),
        tag_uid: normalized_string(source.tag_uid.as_ref()),
        tray_uuid: normalized_string(source.tray_uuid.as_ref()),
        name: normalized_string(source.tray_sub_brands.as_ref()),
        remaining_estimate: normalized_string(source.remain.as_ref()),
        k_value: normalized_string(source.k.as_ref().or(source.k_value.as_ref())),
        toolhead: source
            .toolhead
            .as_ref()
            .and_then(|value| normalized_string(Some(value)))
            .or_else(|| {
                source
                    .extruder_id
                    .as_ref()
                    .and_then(normalize_extruder_toolhead)
            }),
    }
}

fn normalized_number(value: &ScalarValue) -> Option<Number> {
    value.number()
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
    let value = empty_tray_clear_patch(tray_id, global_tray_id(unit_id, &slot.to_string()));
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

fn normalize_active_tray(value: Option<&ScalarValue>) -> Option<ActiveTrayPatch> {
    let tray_now = parse_i64(value?)?;
    match tray_now {
        255 => Some(ActiveTrayPatch::None),
        254 => Some(external_active_tray_patch()),
        0..=15 => Some(ams_active_tray_patch(tray_now)),
        128..=135 => Some(ams_ht_active_tray_patch(tray_now)),
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

fn normalize_multi_color(value: &ColorSource) -> Option<Vec<String>> {
    let colors: Vec<String> = match value {
        ColorSource::List(values) => values.iter().filter_map(normalize_color).collect(),
        ColorSource::Single(ScalarValue::String(raw)) => {
            raw.split(',').filter_map(normalize_color_str).collect()
        }
        ColorSource::Single(value) => normalize_color(value).into_iter().collect(),
    };
    (!colors.is_empty()).then_some(colors)
}

fn filament_switch_installed(value: Option<&ScalarValue>) -> Option<bool> {
    let raw = normalized_string(value)?;
    let parsed =
        u64::from_str_radix(raw.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
    Some((parsed >> 29) & 1 == 1)
}

fn normalize_ams_info(value: &ScalarValue) -> Option<String> {
    let raw = normalized_string(Some(value))?;
    let raw = raw.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(raw, 16).ok()?;
    Some(raw.to_ascii_uppercase())
}

fn normalize_toolhead(
    value: &ScalarValue,
    filament_switch_installed: Option<bool>,
) -> Option<String> {
    let raw = normalized_string(Some(value))?;
    let parsed =
        u64::from_str_radix(raw.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
    match (parsed >> 8) & 0xF {
        0 => Some("R".to_owned()),
        1 => Some("L".to_owned()),
        0xE if filament_switch_installed == Some(true) && matches!((parsed >> 24) & 0xF, 0 | 1) => {
            Some("LR".to_owned())
        }
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
