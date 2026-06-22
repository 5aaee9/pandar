use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    entities::{job_filament_usages, jobs, printer_material_snapshots},
    repositories::{RepositoryError, RepositoryResult, is_sea_orm_unique_violation},
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SlotIdentity {
    slot_index: usize,
    source: &'static str,
    ams_id: Option<String>,
    tray_id: Option<String>,
    global_tray_id: Option<i32>,
    external_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FilamentIdentity {
    filament_id: Option<String>,
    setting_id: Option<String>,
    filament_type: Option<String>,
    color: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Mapping2Entry {
    pub(crate) ams_id: i32,
    pub(crate) slot_id: i32,
}

pub(super) async fn derive_terminal_usage<C>(
    connection: &C,
    job: &jobs::Model,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let identities = mapping_identities(job)?;
    if identities.is_empty() {
        return Ok(());
    }

    let snapshot = printer_material_snapshots::Entity::find()
        .filter(printer_material_snapshots::Column::TenantId.eq(&job.tenant_id))
        .filter(printer_material_snapshots::Column::PrinterId.eq(&job.printer_id))
        .one(connection)
        .await
        .context("failed to load material snapshot for job usage derivation")?;
    let now = pandar_core::created_at_now();

    for identity in identities {
        let filament = snapshot
            .as_ref()
            .and_then(|snapshot| filament_for_identity(snapshot, &identity).transpose())
            .transpose()?
            .unwrap_or_default();
        let model = job_filament_usages::ActiveModel {
            id: Set(uuid::Uuid::new_v4().to_string()),
            tenant_id: Set(job.tenant_id.clone()),
            job_id: Set(job.id.clone()),
            slot_index: Set(i32::try_from(identity.slot_index).expect("slot index should fit i32")),
            source: Set(identity.source.to_string()),
            ams_id: Set(identity.ams_id),
            tray_id: Set(identity.tray_id),
            global_tray_id: Set(identity.global_tray_id),
            external_id: Set(identity.external_id),
            filament_id: Set(filament.filament_id),
            setting_id: Set(filament.setting_id),
            filament_type: Set(filament.filament_type),
            color: Set(filament.color),
            used_mm: Set(None),
            used_grams: Set(None),
            confidence: Set("mapped_no_quantity".to_string()),
            created_at: Set(now.clone()),
        };

        if let Err(err) = model.insert(connection).await {
            if is_sea_orm_unique_violation(
                &err,
                "job_filament_usages.tenant_id, job_filament_usages.job_id, job_filament_usages.slot_index, job_filament_usages.source",
                "job_filament_usages_tenant_id_job_id_slot_index_source_key",
            ) {
                continue;
            }
            return Err(RepositoryError::Database(
                anyhow::Error::new(err).context("failed to insert job filament usage"),
            ));
        }
    }

    Ok(())
}

fn mapping_identities(job: &jobs::Model) -> RepositoryResult<Vec<SlotIdentity>> {
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
            identity_from_mapping2(slot_index, entry)
        } else {
            mapping
                .get(slot_index)
                .and_then(|value| identity_from_mapping(slot_index, *value))
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

fn identity_from_mapping(slot_index: usize, value: i32) -> Option<SlotIdentity> {
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

fn identity_from_mapping2(slot_index: usize, entry: &Mapping2Entry) -> Option<SlotIdentity> {
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
            global_tray_id: entry
                .ams_id
                .checked_mul(4)
                .and_then(|global| global.checked_add(slot_id)),
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

fn filament_for_identity(
    snapshot: &printer_material_snapshots::Model,
    identity: &SlotIdentity,
) -> RepositoryResult<Option<FilamentIdentity>> {
    let ams_units: Vec<Value> = serde_json::from_str(&snapshot.ams_json)
        .context("failed to parse material AMS snapshot for job usage derivation")?;
    let external_spools: Vec<Value> = serde_json::from_str(&snapshot.external_spools_json)
        .context("failed to parse material external spool snapshot for job usage derivation")?;

    if identity.external_id.is_some() {
        return Ok(external_spools
            .iter()
            .find(|spool| {
                field_string(spool, "external_id").as_deref() == identity.external_id.as_deref()
                    && field_string(spool, "tray_id").as_deref() == identity.tray_id.as_deref()
            })
            .map(filament_from_value));
    }

    Ok(ams_units
        .iter()
        .find(|unit| field_string(unit, "unit_id").as_deref() == identity.ams_id.as_deref())
        .and_then(|unit| unit.get("trays").and_then(Value::as_array))
        .and_then(|trays| {
            trays.iter().find(|tray| {
                field_string(tray, "tray_id").as_deref() == identity.tray_id.as_deref()
                    || field_i64(tray, "global_tray_id")
                        .zip(identity.global_tray_id)
                        .is_some_and(|(left, right)| left == i64::from(right))
            })
        })
        .map(filament_from_value))
}

fn filament_from_value(value: &Value) -> FilamentIdentity {
    FilamentIdentity {
        filament_id: field_string(value, "filament_id"),
        setting_id: field_string(value, "setting_id"),
        filament_type: field_string(value, "type"),
        color: field_string(value, "color"),
    }
}

fn field_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn field_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|value| match value {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    })
}
