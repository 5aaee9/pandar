use serde_json::{Map, Value};

use super::{apply_material_fields, normalize_extruder_toolhead, normalized_string};

pub(super) struct ExternalSpoolsPatch {
    pub(super) spools: Vec<Value>,
    pub(super) replace: bool,
}

pub(super) fn normalize_external_spools(print: &Value, ams: &Value) -> Option<ExternalSpoolsPatch> {
    if let Some(vir_slot) = print.get("vir_slot").or_else(|| ams.get("vir_slot")) {
        return normalize_external_source(vir_slot, true);
    }
    print
        .get("vt_tray")
        .or_else(|| ams.get("vt_tray"))
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
    normalized.insert(
        "external_id".to_owned(),
        Value::String(normalize_external_id(spool, index, multi)),
    );
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

pub(super) fn has_dual_external_slots(value: &Value) -> bool {
    value
        .get("vir_slot")
        .or_else(|| value.get("vt_tray"))
        .and_then(Value::as_array)
        .is_some_and(|slots| {
            let has_main = slots
                .iter()
                .any(|slot| external_slot_id(slot).as_deref() == Some("255"));
            let has_deputy = slots
                .iter()
                .any(|slot| external_slot_id(slot).as_deref() == Some("254"));
            has_main && has_deputy
        })
}

fn external_slot_id(slot: &Value) -> Option<String> {
    slot.get("id")
        .or_else(|| slot.get("external_id"))
        .and_then(|value| normalized_string(Some(value)))
}

fn normalize_external_id(spool: &Value, index: usize, multi: bool) -> String {
    if let Some(toolhead) = spool
        .get("toolhead")
        .and_then(|value| normalized_string(Some(value)))
        .or_else(|| {
            spool
                .get("extruder_id")
                .and_then(normalize_extruder_toolhead)
        })
    {
        return external_id_for_toolhead(&toolhead);
    }

    if let Some(id) = spool
        .get("external_id")
        .or_else(|| spool.get("id"))
        .and_then(|value| normalized_string(Some(value)))
        && matches!(id.as_str(), "254" | "255")
    {
        return id;
    }

    if multi && index == 0 {
        "255".to_owned()
    } else {
        "254".to_owned()
    }
}

fn external_id_for_toolhead(toolhead: &str) -> String {
    if toolhead == "L" || toolhead == "l" {
        "254".to_owned()
    } else {
        "255".to_owned()
    }
}
