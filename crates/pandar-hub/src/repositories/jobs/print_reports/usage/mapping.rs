use anyhow::Context;
use serde::Deserialize;

use super::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping2Entry {
    ams_id: i32,
    slot_id: i32,
}

pub(super) fn mapping_identities(
    job: &jobs::Model,
    ams_units: &[MaterialUnit],
) -> RepositoryResult<Vec<SlotIdentity>> {
    let mapping = job
        .ams_mapping_json
        .as_deref()
        .map(parse_mapping)
        .transpose()?
        .unwrap_or_default();
    let mapping2 = job
        .ams_mapping2_json
        .as_deref()
        .map(parse_mapping2)
        .transpose()?
        .unwrap_or_default();
    let slots = mapping.len().max(mapping2.len());
    let mut identities = Vec::new();

    for slot_index in 0..slots {
        let identity = if let Some(entry) = mapping2.get(slot_index) {
            identity_from_mapping2(slot_index, entry, ams_units)
        } else {
            mapping
                .get(slot_index)
                .and_then(|value| identity_from_mapping(slot_index, *value, ams_units))
        };
        if let Some(identity) = identity {
            identities.push(identity);
        }
    }

    Ok(identities)
}

fn parse_mapping(json: &str) -> RepositoryResult<Vec<i32>> {
    serde_json::from_str(json)
        .with_context(|| "failed to parse persisted ams_mapping_json")
        .map_err(Into::into)
}

fn parse_mapping2(json: &str) -> RepositoryResult<Vec<Mapping2Entry>> {
    serde_json::from_str(json)
        .with_context(|| "failed to parse persisted ams_mapping2_json")
        .map_err(Into::into)
}

fn identity_from_mapping(
    slot_index: usize,
    value: i32,
    ams_units: &[MaterialUnit],
) -> Option<SlotIdentity> {
    match value {
        -1 | 255 => None,
        0..=15 => Some(SlotIdentity {
            slot_index,
            source: "ams_mapping",
            ams_id: Some((value / 4).to_string()),
            tray_id: Some((value % 4).to_string()),
            global_tray_id: Some(value),
            external_id: None,
        }),
        24..=27 => identity_for_global_tray(slot_index, value, ams_units),
        128..=135 => Some(SlotIdentity {
            slot_index,
            source: "ams_mapping",
            ams_id: Some(value.to_string()),
            tray_id: Some("0".to_string()),
            global_tray_id: None,
            external_id: None,
        }),
        254 => Some(SlotIdentity {
            slot_index,
            source: "ams_mapping",
            ams_id: None,
            tray_id: Some("0".to_string()),
            global_tray_id: None,
            external_id: Some("254".to_string()),
        }),
        _ => None,
    }
}

fn identity_from_mapping2(
    slot_index: usize,
    entry: &Mapping2Entry,
    ams_units: &[MaterialUnit],
) -> Option<SlotIdentity> {
    match (entry.ams_id, entry.slot_id) {
        (_, 255) => None,
        (254 | 255, slot_id) => Some(SlotIdentity {
            slot_index,
            source: "ams_mapping2",
            ams_id: None,
            tray_id: Some(slot_id.to_string()),
            global_tray_id: None,
            external_id: Some("254".to_string()),
        }),
        (_, slot_id) if !(0..=3).contains(&slot_id) => None,
        (0..=63, slot_id) => Some(SlotIdentity {
            slot_index,
            source: "ams_mapping2",
            ams_id: Some(entry.ams_id.to_string()),
            tray_id: Some(slot_id.to_string()),
            global_tray_id: global_tray_for_route(ams_units, entry.ams_id, slot_id).or_else(|| {
                entry
                    .ams_id
                    .checked_mul(4)
                    .and_then(|global| global.checked_add(slot_id))
            }),
            external_id: None,
        }),
        (128..=135, slot_id) => Some(SlotIdentity {
            slot_index,
            source: "ams_mapping2",
            ams_id: Some(entry.ams_id.to_string()),
            tray_id: Some(slot_id.to_string()),
            global_tray_id: None,
            external_id: None,
        }),
        _ => None,
    }
}

fn identity_for_global_tray(
    slot_index: usize,
    global_tray_id: i32,
    ams_units: &[MaterialUnit],
) -> Option<SlotIdentity> {
    ams_units.iter().find_map(|unit| {
        let ams_id = unit.unit_id.as_ref().and_then(ScalarField::string)?;
        unit.trays.iter().find_map(|tray| {
            (tray.global_tray_id.as_ref().and_then(ScalarField::i64)
                == Some(i64::from(global_tray_id)))
            .then(|| SlotIdentity {
                slot_index,
                source: "ams_mapping",
                ams_id: Some(ams_id.clone()),
                tray_id: tray.tray_id.as_ref().and_then(ScalarField::string),
                global_tray_id: Some(global_tray_id),
                external_id: None,
            })
        })
    })
}

fn global_tray_for_route(ams_units: &[MaterialUnit], ams_id: i32, tray_id: i32) -> Option<i32> {
    let ams_id = ams_id.to_string();
    let tray_id = tray_id.to_string();
    ams_units
        .iter()
        .find(|unit| {
            unit.unit_id
                .as_ref()
                .and_then(ScalarField::string)
                .as_deref()
                == Some(&ams_id)
        })
        .and_then(|unit| {
            unit.trays.iter().find(|tray| {
                tray.tray_id
                    .as_ref()
                    .and_then(ScalarField::string)
                    .as_deref()
                    == Some(&tray_id)
            })
        })
        .and_then(|tray| tray.global_tray_id.as_ref())
        .and_then(ScalarField::i64)
        .and_then(|global| i32::try_from(global).ok())
}
