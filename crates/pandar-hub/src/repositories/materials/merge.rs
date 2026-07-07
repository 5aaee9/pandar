use serde::Deserialize;
use serde_json::Value;

use crate::entities::printer_material_snapshots;

use super::patch::{
    MaterialExternalSpoolPatch, MaterialTrayPatch, MaterialUnitPatch, ParsedPatch, Presence,
    parse_array_json, parse_object_json,
};

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

fn merge_units(current: &mut Vec<Value>, patches: &[MaterialUnitPatch]) {
    for patch in patches {
        let Some(unit_id) = patch.unit_id.as_deref() else {
            continue;
        };
        let patch_object = patch.object_without_control_fields();

        if let Some(current_unit) = current
            .iter_mut()
            .find(|unit| unit_key(unit).as_deref() == Some(unit_id))
        {
            merge_object_fields(current_unit, &patch_object);
            if let Some(trays) = &patch.trays {
                merge_trays(current_unit, trays, patch.replace_trays);
            }
        } else {
            let mut created = patch.value_without_null_fields();
            if let Value::Object(object) = &mut created {
                object
                    .entry("trays".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
            }
            current.push(created);
        }
    }
    current.sort_by_key(unit_key);
}

fn merge_external_spools(
    current: &mut Vec<Value>,
    patches: &[MaterialExternalSpoolPatch],
    replace: bool,
) {
    for patch in patches {
        let Some(patch_key) = patch.key() else {
            continue;
        };
        let patch_value = patch.value_without_null_fields();
        if let Some(current_spool) = current
            .iter_mut()
            .find(|spool| external_key(spool).as_ref() == Some(&patch_key))
        {
            merge_object_fields(current_spool, &patch.value_with_null_fields());
        } else {
            current.push(patch_value);
        }
    }
    if replace {
        let patch_keys = patches
            .iter()
            .filter_map(MaterialExternalSpoolPatch::key)
            .collect::<Vec<_>>();
        current.retain(|spool| {
            external_key(spool)
                .map(|key| patch_keys.contains(&key))
                .unwrap_or(false)
        });
    }
    current.sort_by_key(external_key);
}

fn merge_trays(parent: &mut Value, patches: &[MaterialTrayPatch], replace: bool) {
    let object = parent.as_object_mut().expect("unit state should be object");
    let current = object
        .entry("trays")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("nested material collection should be array");
    for patch in patches {
        let Some(patch_key) = patch.tray_id.as_deref() else {
            continue;
        };
        let patch_value = patch.value_without_null_fields();
        if let Some(existing) = current
            .iter_mut()
            .find(|entry| tray_key(entry).as_deref() == Some(patch_key))
        {
            merge_object_fields(existing, &patch.value_with_null_fields());
        } else {
            current.push(patch_value);
        }
    }
    if replace {
        let patch_keys = patches
            .iter()
            .filter_map(|patch| patch.tray_id.as_deref())
            .collect::<Vec<_>>();
        current.retain(|entry| {
            tray_key(entry)
                .map(|key| patch_keys.contains(&key.as_str()))
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

#[derive(Deserialize)]
struct UnitIdentity {
    unit_id: Option<String>,
}

#[derive(Deserialize)]
struct TrayIdentity {
    tray_id: Option<String>,
}

#[derive(Deserialize)]
struct ExternalIdentity {
    external_id: Option<String>,
    tray_id: Option<String>,
}

fn unit_key(value: &Value) -> Option<String> {
    serde_json::from_value::<UnitIdentity>(value.clone())
        .ok()?
        .unit_id
}

fn tray_key(value: &Value) -> Option<String> {
    serde_json::from_value::<TrayIdentity>(value.clone())
        .ok()?
        .tray_id
}

fn external_key(value: &Value) -> Option<(String, String)> {
    let identity = serde_json::from_value::<ExternalIdentity>(value.clone()).ok()?;
    Some((identity.external_id?, identity.tray_id?))
}
