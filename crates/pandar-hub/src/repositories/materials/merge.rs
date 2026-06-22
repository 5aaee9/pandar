use serde_json::Value;

use crate::entities::printer_material_snapshots;

use super::patch::{ParsedPatch, Presence, parse_array_json, parse_object_json};

pub(super) struct MergedSnapshot {
    pub(super) ams_units: Value,
    pub(super) external_spools: Value,
    pub(super) active_tray: Option<Value>,
}

pub(super) fn merge_snapshot(
    current: Option<&printer_material_snapshots::Model>,
    patch: &ParsedPatch,
) -> anyhow::Result<MergedSnapshot> {
    let mut ams_units = current
        .map(|snapshot| parse_array_json(&snapshot.ams_json, "persisted AMS material state"))
        .transpose()?
        .unwrap_or_default();
    if let Some(units) = &patch.ams_units {
        merge_units(&mut ams_units, units);
    }

    let mut external_spools = current
        .map(|snapshot| {
            parse_array_json(
                &snapshot.external_spools_json,
                "persisted external spool material state",
            )
        })
        .transpose()?
        .unwrap_or_default();
    if let Some(spools) = &patch.external_spools {
        merge_external_spools(&mut external_spools, spools, patch.replace_external_spools);
    }

    let active_tray = match &patch.active_tray {
        Presence::Absent => current
            .and_then(|snapshot| snapshot.active_tray_json.as_ref())
            .map(|json| parse_object_json(json, "persisted active material tray"))
            .transpose()?,
        Presence::Null => None,
        Presence::Value(value) => Some(value.clone()),
    };

    Ok(MergedSnapshot {
        ams_units: Value::Array(ams_units),
        external_spools: Value::Array(external_spools),
        active_tray,
    })
}

fn merge_units(current: &mut Vec<Value>, patches: &[Value]) {
    for patch in patches {
        let Some(unit_id) = patch.get("unit_id").and_then(Value::as_str) else {
            continue;
        };
        let replace_trays = patch
            .get("replace_trays")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let patch_trays = patch.get("trays").and_then(Value::as_array).cloned();
        let patch_object = strip_control_fields(patch, &["trays", "replace_trays"]);

        if let Some(current_unit) = current
            .iter_mut()
            .find(|unit| unit.get("unit_id").and_then(Value::as_str) == Some(unit_id))
        {
            merge_object_fields(current_unit, &patch_object);
            if let Some(trays) = patch_trays {
                merge_nested_array(current_unit, "trays", &trays, tray_key, replace_trays);
            }
        } else {
            let mut created = object_without_nulls(&patch_object);
            if let Value::Object(object) = &mut created {
                object.insert(
                    "trays".to_string(),
                    Value::Array(
                        patch_trays
                            .unwrap_or_default()
                            .into_iter()
                            .map(|tray| object_without_nulls(&tray))
                            .collect(),
                    ),
                );
            }
            current.push(created);
        }
    }
    current.sort_by_key(|unit| identity(unit, "unit_id"));
}

fn merge_external_spools(current: &mut Vec<Value>, patches: &[Value], replace: bool) {
    for patch in patches {
        let Some(patch_key) = external_key(patch) else {
            continue;
        };
        if let Some(current_spool) = current
            .iter_mut()
            .find(|spool| external_key(spool).as_ref() == Some(&patch_key))
        {
            merge_object_fields(current_spool, patch);
        } else {
            current.push(object_without_nulls(patch));
        }
    }
    if replace {
        let patch_keys = patches.iter().filter_map(external_key).collect::<Vec<_>>();
        current.retain(|spool| {
            external_key(spool)
                .map(|key| patch_keys.contains(&key))
                .unwrap_or(false)
        });
    }
    current.sort_by_key(external_key);
}

fn merge_nested_array<F>(parent: &mut Value, key: &str, patches: &[Value], key_fn: F, replace: bool)
where
    F: Fn(&Value) -> Option<String> + Copy,
{
    let object = parent.as_object_mut().expect("unit state should be object");
    let current = object
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("nested material collection should be array");
    for patch in patches {
        let Some(patch_key) = key_fn(patch) else {
            continue;
        };
        if let Some(existing) = current
            .iter_mut()
            .find(|entry| key_fn(entry).as_ref() == Some(&patch_key))
        {
            merge_object_fields(existing, patch);
        } else {
            current.push(object_without_nulls(patch));
        }
    }
    if replace {
        let patch_keys = patches.iter().filter_map(key_fn).collect::<Vec<_>>();
        current.retain(|entry| {
            key_fn(entry)
                .map(|key| patch_keys.contains(&key))
                .unwrap_or(false)
        });
    }
    current.sort_by_key(tray_key);
}

fn merge_object_fields(current: &mut Value, patch: &Value) {
    let Some(current_object) = current.as_object_mut() else {
        return;
    };
    let Some(patch_object) = patch.as_object() else {
        return;
    };

    for (key, value) in patch_object {
        if value.is_null() {
            current_object.remove(key);
        } else {
            current_object.insert(key.clone(), value.clone());
        }
    }
}

fn object_without_nulls(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };
    Value::Object(
        object
            .iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn strip_control_fields(value: &Value, controls: &[&str]) -> Value {
    let mut value = value.clone();
    if let Value::Object(object) = &mut value {
        for control in controls {
            object.remove(*control);
        }
    }
    value
}

fn identity(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn tray_key(value: &Value) -> Option<String> {
    identity(value, "tray_id")
}

fn external_key(value: &Value) -> Option<(String, String)> {
    Some((identity(value, "external_id")?, identity(value, "tray_id")?))
}
