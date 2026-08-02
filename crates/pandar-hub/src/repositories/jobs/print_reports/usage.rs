mod mapping;

use anyhow::Context;
use mapping::mapping_identities;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};
use serde::Deserialize;

use crate::{
    db::{UniqueConstraint, is_unique_violation},
    entities::{job_filament_usages, jobs, printer_material_snapshots},
    repositories::{RepositoryError, RepositoryResult},
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
struct MaterialUnit {
    unit_id: Option<ScalarField>,
    #[serde(default)]
    trays: Vec<MaterialTray>,
}

#[derive(Debug, Deserialize)]
struct MaterialTray {
    tray_id: Option<ScalarField>,
    global_tray_id: Option<ScalarField>,
    filament_id: Option<ScalarField>,
    setting_id: Option<ScalarField>,
    #[serde(rename = "type")]
    filament_type: Option<ScalarField>,
    color: Option<ScalarField>,
}

#[derive(Debug, Deserialize)]
struct ExternalSpool {
    external_id: Option<ScalarField>,
    tray_id: Option<ScalarField>,
    filament_id: Option<ScalarField>,
    setting_id: Option<ScalarField>,
    #[serde(rename = "type")]
    filament_type: Option<ScalarField>,
    color: Option<ScalarField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ScalarField {
    String(String),
    I64(i64),
    U64(u64),
    F64(f64),
}

pub(super) async fn derive_terminal_usage<C>(
    connection: &C,
    job: &jobs::Model,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let snapshot = printer_material_snapshots::Entity::find()
        .filter(printer_material_snapshots::Column::TenantId.eq(&job.tenant_id))
        .filter(printer_material_snapshots::Column::PrinterId.eq(&job.printer_id))
        .one(connection)
        .await
        .context("failed to load material snapshot for job usage derivation")?;
    let ams_units: Vec<MaterialUnit> = snapshot
        .as_ref()
        .map(|snapshot| {
            serde_json::from_str(&snapshot.ams_json)
                .context("failed to parse material AMS snapshot for job usage routing")
        })
        .transpose()?
        .unwrap_or_default();
    let identities = mapping_identities(job, &ams_units)?;
    if identities.is_empty() {
        return Ok(());
    }
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
            if is_unique_violation(&err, UniqueConstraint::JobFilamentUsageSlot) {
                continue;
            }
            return Err(RepositoryError::Database(
                anyhow::Error::new(err).context("failed to insert job filament usage"),
            ));
        }
    }

    Ok(())
}

fn filament_for_identity(
    snapshot: &printer_material_snapshots::Model,
    identity: &SlotIdentity,
) -> RepositoryResult<Option<FilamentIdentity>> {
    let ams_units: Vec<MaterialUnit> = serde_json::from_str(&snapshot.ams_json)
        .context("failed to parse material AMS snapshot for job usage derivation")?;
    let external_spools: Vec<ExternalSpool> = serde_json::from_str(&snapshot.external_spools_json)
        .context("failed to parse material external spool snapshot for job usage derivation")?;

    if identity.external_id.is_some() {
        return Ok(external_spools
            .iter()
            .find(|spool| {
                spool
                    .external_id
                    .as_ref()
                    .and_then(ScalarField::string)
                    .as_deref()
                    == identity.external_id.as_deref()
                    && spool
                        .tray_id
                        .as_ref()
                        .and_then(ScalarField::string)
                        .as_deref()
                        == identity.tray_id.as_deref()
            })
            .map(FilamentIdentity::from));
    }

    Ok(ams_units
        .iter()
        .find(|unit| {
            unit.unit_id
                .as_ref()
                .and_then(ScalarField::string)
                .as_deref()
                == identity.ams_id.as_deref()
        })
        .and_then(|unit| {
            unit.trays.iter().find(|tray| {
                tray.tray_id
                    .as_ref()
                    .and_then(ScalarField::string)
                    .as_deref()
                    == identity.tray_id.as_deref()
                    || tray
                        .global_tray_id
                        .as_ref()
                        .and_then(ScalarField::i64)
                        .zip(identity.global_tray_id)
                        .is_some_and(|(left, right)| left == i64::from(right))
            })
        })
        .map(FilamentIdentity::from))
}

impl From<&MaterialTray> for FilamentIdentity {
    fn from(value: &MaterialTray) -> Self {
        Self {
            filament_id: value.filament_id.as_ref().and_then(ScalarField::string),
            setting_id: value.setting_id.as_ref().and_then(ScalarField::string),
            filament_type: value.filament_type.as_ref().and_then(ScalarField::string),
            color: value.color.as_ref().and_then(ScalarField::string),
        }
    }
}

impl From<&ExternalSpool> for FilamentIdentity {
    fn from(value: &ExternalSpool) -> Self {
        Self {
            filament_id: value.filament_id.as_ref().and_then(ScalarField::string),
            setting_id: value.setting_id.as_ref().and_then(ScalarField::string),
            filament_type: value.filament_type.as_ref().and_then(ScalarField::string),
            color: value.color.as_ref().and_then(ScalarField::string),
        }
    }
}

impl ScalarField {
    fn string(&self) -> Option<String> {
        match self {
            Self::String(value) => Some(value.clone()),
            Self::I64(value) => Some(value.to_string()),
            Self::U64(value) => Some(value.to_string()),
            Self::F64(value) if value.is_finite() => Some(value.to_string()),
            Self::F64(_) => None,
        }
    }

    fn i64(&self) -> Option<i64> {
        match self {
            Self::String(value) => value.parse().ok(),
            Self::I64(value) => Some(*value),
            Self::U64(value) => i64::try_from(*value).ok(),
            Self::F64(value)
                if value.fract() == 0.0
                    && *value >= i64::MIN as f64
                    && *value <= i64::MAX as f64 =>
            {
                Some(*value as i64)
            }
            Self::F64(_) => None,
        }
    }
}
