use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::entities::printer_material_snapshots;

use super::patch::{
    MaterialExternalSpoolPatch, MaterialTrayPatch, MaterialUnitPatch, ParsedPatch, Presence,
    parse_object_json,
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
        .map(|snapshot| parse_units_json(&snapshot.ams_json, "persisted AMS material state"))
        .transpose()?
        .unwrap_or_default();
    if let Some(units) = &patch.ams_units {
        merge_units(&mut ams_units, units);
    }

    let mut external_spools = current
        .map(|snapshot| {
            parse_external_spools_json(
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
        ams_units: serde_json::to_value(ams_units)
            .context("failed to serialize AMS merge state")?,
        external_spools: serde_json::to_value(external_spools)
            .context("failed to serialize external spool merge state")?,
        active_tray,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MaterialUnitState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unit_id: Option<String>,
    #[serde(default)]
    trays: Vec<MaterialTrayState>,
    #[serde(default, flatten)]
    fields: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MaterialTrayState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tray_id: Option<String>,
    #[serde(default, flatten)]
    fields: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MaterialExternalSpoolState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    external_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tray_id: Option<String>,
    #[serde(default, flatten)]
    fields: Map<String, Value>,
}

fn parse_units_json(raw: &str, context: &str) -> anyhow::Result<Vec<MaterialUnitState>> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse {context}"))
}

fn parse_external_spools_json(
    raw: &str,
    context: &str,
) -> anyhow::Result<Vec<MaterialExternalSpoolState>> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse {context}"))
}

fn merge_units(current: &mut Vec<MaterialUnitState>, patches: &[MaterialUnitPatch]) {
    for patch in patches {
        let Some(unit_id) = patch.unit_id.as_deref() else {
            continue;
        };

        if let Some(current_unit) = current
            .iter_mut()
            .find(|unit| unit.unit_id.as_deref() == Some(unit_id))
        {
            merge_fields(&mut current_unit.fields, patch.fields_with_nulls());
            if let Some(trays) = &patch.trays {
                merge_trays(&mut current_unit.trays, trays, patch.replace_trays);
            }
        } else {
            current.push(MaterialUnitState {
                unit_id: Some(unit_id.to_owned()),
                trays: patch
                    .trays
                    .as_deref()
                    .map(new_tray_states)
                    .unwrap_or_default(),
                fields: patch.fields_without_nulls_for_new_unit(),
            });
        }
    }
    current.sort_by_key(|unit| unit.unit_id.clone());
}

fn merge_external_spools(
    current: &mut Vec<MaterialExternalSpoolState>,
    patches: &[MaterialExternalSpoolPatch],
    replace: bool,
) {
    for patch in patches {
        let Some(patch_key) = patch.key() else {
            continue;
        };
        if let Some(current_spool) = current
            .iter_mut()
            .find(|spool| spool.key().as_ref() == Some(&patch_key))
        {
            merge_fields(&mut current_spool.fields, patch.fields_with_nulls());
        } else {
            current.push(MaterialExternalSpoolState {
                external_id: Some(patch_key.0),
                tray_id: Some(patch_key.1),
                fields: patch.fields_without_nulls(),
            });
        }
    }
    if replace {
        let patch_keys = patches
            .iter()
            .filter_map(MaterialExternalSpoolPatch::key)
            .collect::<Vec<_>>();
        current.retain(|spool| {
            spool
                .key()
                .map(|key| patch_keys.contains(&key))
                .unwrap_or(false)
        });
    }
    current.sort_by_key(MaterialExternalSpoolState::key);
}

fn merge_trays(current: &mut Vec<MaterialTrayState>, patches: &[MaterialTrayPatch], replace: bool) {
    for patch in patches {
        let Some(patch_key) = patch.tray_id.as_deref() else {
            continue;
        };
        if let Some(existing) = current
            .iter_mut()
            .find(|entry| entry.tray_id.as_deref() == Some(patch_key))
        {
            merge_fields(&mut existing.fields, patch.fields_with_nulls());
        } else {
            current.push(MaterialTrayState {
                tray_id: Some(patch_key.to_owned()),
                fields: patch.fields_without_nulls(),
            });
        }
    }
    if replace {
        let patch_keys = patches
            .iter()
            .filter_map(|patch| patch.tray_id.as_deref())
            .collect::<Vec<_>>();
        current.retain(|entry| {
            entry
                .tray_id
                .as_ref()
                .map(|key| patch_keys.contains(&key.as_str()))
                .unwrap_or(false)
        });
    }
    current.sort_by_key(|tray| tray.tray_id.clone());
}

fn new_tray_states(patches: &[MaterialTrayPatch]) -> Vec<MaterialTrayState> {
    let mut trays = patches
        .iter()
        .filter_map(|patch| {
            Some(MaterialTrayState {
                tray_id: Some(patch.tray_id.clone()?),
                fields: patch.fields_without_nulls(),
            })
        })
        .collect::<Vec<_>>();
    trays.sort_by_key(|tray| tray.tray_id.clone());
    trays
}

fn merge_fields(current: &mut Map<String, Value>, patch: Map<String, Value>) {
    for (key, value) in patch {
        if value.is_null() {
            current.remove(&key);
        } else {
            current.insert(key, value);
        }
    }
}

impl MaterialExternalSpoolState {
    fn key(&self) -> Option<(String, String)> {
        Some((self.external_id.clone()?, self.tray_id.clone()?))
    }
}
