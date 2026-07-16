use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
#[cfg(test)]
use sea_orm::{SqliteTransactionMode, TransactionOptions, TransactionTrait};

use crate::{
    db::Database,
    entities::{printer_material_snapshots, printers},
    repositories::{RepositoryError, RepositoryResult, begin_current_agent_transaction},
};

mod merge;
mod patch;
#[cfg(test)]
mod test_json;

use merge::merge_snapshot;
pub use patch::MaterialJsonValue;
#[cfg(test)]
use patch::sanitize_message;
use patch::{is_older, parse_array_json, parse_object_json, parse_patch_result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialPatchInput {
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub printer_id: String,
    pub serial_number: String,
    pub printer_materials_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialSnapshot {
    pub id: String,
    pub tenant_id: TenantId,
    pub printer_id: String,
    pub agent_id: AgentId,
    pub serial_number: String,
    pub ams_units: MaterialJsonValue,
    pub external_spools: MaterialJsonValue,
    pub active_tray: Option<MaterialJsonValue>,
    pub filament_switch_installed: Option<bool>,
    pub cfg: Option<String>,
    pub aux: Option<String>,
    pub stat: Option<String>,
    pub observed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialPatchOutcome {
    Empty,
    Invalid { error: String },
    Older,
    Unchanged(MaterialSnapshot),
    Changed(MaterialSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CurrentMaterialPatchOutcome {
    MissingPrinter,
    SerialMismatch {
        printer_id: String,
        printer_serial: String,
    },
    Applied {
        printer_id: String,
        outcome: Box<MaterialPatchOutcome>,
    },
}

#[derive(Debug, Clone)]
pub struct MaterialRepository {
    database: Database,
}

impl MaterialRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn latest_for_printer(
        &self,
        tenant_id: TenantId,
        printer_id: &str,
    ) -> RepositoryResult<Option<MaterialSnapshot>> {
        printer_material_snapshots::Entity::find()
            .filter(printer_material_snapshots::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_material_snapshots::Column::PrinterId.eq(printer_id))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to load latest material snapshot")?
            .map(snapshot_from_model)
            .transpose()
    }

    pub async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepositoryResult<Vec<MaterialSnapshot>> {
        printer_material_snapshots::Entity::find()
            .filter(printer_material_snapshots::Column::TenantId.eq(tenant_id.to_string()))
            .order_by_asc(printer_material_snapshots::Column::SerialNumber)
            .order_by_asc(printer_material_snapshots::Column::PrinterId)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list material snapshots")?
            .into_iter()
            .map(snapshot_from_model)
            .collect()
    }

    pub(crate) async fn clear_for_printer_if_current(
        &self,
        session_id: &str,
        tenant_id: TenantId,
        agent_id: AgentId,
        printer_id: &str,
    ) -> RepositoryResult<()> {
        let tx = begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
            .await?;
        printer_material_snapshots::Entity::delete_many()
            .filter(printer_material_snapshots::Column::TenantId.eq(tenant_id.to_string()))
            .filter(printer_material_snapshots::Column::PrinterId.eq(printer_id))
            .exec(&tx)
            .await
            .context("failed to clear material snapshot for authoritative printer connection")?;
        tx.commit()
            .await
            .context("failed to commit authoritative material snapshot clear")?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn upsert_from_patch(
        &self,
        input: MaterialPatchInput,
    ) -> RepositoryResult<Option<MaterialSnapshot>> {
        match self.upsert_from_patch_outcome(input).await? {
            MaterialPatchOutcome::Changed(snapshot) | MaterialPatchOutcome::Unchanged(snapshot) => {
                Ok(Some(snapshot))
            }
            MaterialPatchOutcome::Invalid { error } => {
                tracing::warn!(error = %sanitize_message(&error), "ignored material patch");
                Ok(None)
            }
            MaterialPatchOutcome::Empty | MaterialPatchOutcome::Older => Ok(None),
        }
    }

    #[cfg(test)]
    pub(crate) async fn upsert_from_patch_outcome(
        &self,
        input: MaterialPatchInput,
    ) -> RepositoryResult<MaterialPatchOutcome> {
        let connection = self.database.sea_orm_connection();
        let tx = connection
            .begin_with_options(TransactionOptions {
                sqlite_transaction_mode: matches!(self.database, Database::Sqlite(_))
                    .then_some(SqliteTransactionMode::Immediate),
                ..Default::default()
            })
            .await
            .context("failed to begin test material snapshot transaction")?;
        let outcome = upsert_from_patch_outcome_in_connection(&tx, input).await?;
        tx.commit()
            .await
            .context("failed to commit test material snapshot transaction")?;
        Ok(outcome)
    }

    pub(crate) async fn apply_snapshot_if_current(
        &self,
        session_id: &str,
        tenant_id: TenantId,
        agent_id: AgentId,
        requested_printer_id: &str,
        serial_number: String,
        printer_materials_json: String,
    ) -> RepositoryResult<CurrentMaterialPatchOutcome> {
        let tx = begin_current_agent_transaction(&self.database, tenant_id, agent_id, session_id)
            .await?;
        let query = if requested_printer_id.is_empty() {
            printers::Entity::find()
                .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
                .filter(printers::Column::AgentId.eq(agent_id.to_string()))
                .filter(printers::Column::SerialNumber.eq(&serial_number))
        } else {
            printers::Entity::find_by_id(requested_printer_id)
                .filter(printers::Column::TenantId.eq(tenant_id.to_string()))
                .filter(printers::Column::AgentId.eq(agent_id.to_string()))
        };
        let printer = match tx.get_database_backend() {
            sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(&tx).await,
            _ => query.one(&tx).await,
        }
        .context("failed to lock material snapshot printer")?;
        let result = match printer {
            None => CurrentMaterialPatchOutcome::MissingPrinter,
            Some(printer) if printer.serial_number != serial_number => {
                CurrentMaterialPatchOutcome::SerialMismatch {
                    printer_id: printer.id,
                    printer_serial: printer.serial_number,
                }
            }
            Some(printer) => {
                let printer_id = printer.id;
                let outcome = upsert_from_patch_outcome_in_connection(
                    &tx,
                    MaterialPatchInput {
                        tenant_id,
                        agent_id,
                        printer_id: printer_id.clone(),
                        serial_number,
                        printer_materials_json,
                    },
                )
                .await?;
                CurrentMaterialPatchOutcome::Applied {
                    printer_id,
                    outcome: Box::new(outcome),
                }
            }
        };
        tx.commit()
            .await
            .context("failed to commit current-session material snapshot transaction")?;
        Ok(result)
    }
}

pub(crate) async fn upsert_from_patch_outcome_in_connection<C>(
    connection: &C,
    input: MaterialPatchInput,
) -> RepositoryResult<MaterialPatchOutcome>
where
    C: sea_orm::ConnectionTrait,
{
    if input.printer_materials_json.trim().is_empty() {
        return Ok(MaterialPatchOutcome::Empty);
    }
    let patch = match parse_patch_result(&input.printer_materials_json)
        .context("invalid material patch JSON")
    {
        Ok(patch) => patch,
        Err(err) => {
            return Ok(MaterialPatchOutcome::Invalid {
                error: format!("{err:#}"),
            });
        }
    };

    let query = printers::Entity::find_by_id(&input.printer_id)
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(input.agent_id.to_string()));
    let Some(printer) = (match connection.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(connection).await,
        _ => query.one(connection).await,
    })
    .context("failed to verify material snapshot printer ownership")?
    else {
        return Err(RepositoryError::MissingPrinter);
    };

    let current = printer_material_snapshots::Entity::find()
        .filter(printer_material_snapshots::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printer_material_snapshots::Column::PrinterId.eq(&input.printer_id))
        .one(connection)
        .await
        .context("failed to load existing material snapshot")?;

    if let Some(current) = &current
        && is_older(&patch.observed_at, &current.observed_at)?
    {
        return Ok(MaterialPatchOutcome::Older);
    }

    let current_snapshot = current.clone().map(snapshot_from_model).transpose()?;
    let merged = merge_snapshot(current.as_ref(), &patch)?;
    if let Some(snapshot) = current_snapshot
        && snapshot.observed_at == patch.observed_at
        && snapshot.ams_units == merged.ams_units
        && snapshot.external_spools == merged.external_spools
        && snapshot.active_tray == merged.active_tray
        && snapshot.filament_switch_installed == merged.filament_switch_installed
        && snapshot.cfg == merged.cfg
        && snapshot.aux == merged.aux
        && snapshot.stat == merged.stat
    {
        return Ok(MaterialPatchOutcome::Unchanged(snapshot));
    }

    let now = pandar_core::created_at_now();
    let id = current
        .as_ref()
        .map(|snapshot| snapshot.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let serial_number = if input.serial_number.is_empty() {
        printer.serial_number
    } else {
        input.serial_number
    };

    let active_tray_json = merged
        .active_tray
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("failed to serialize active material tray")?;
    let model = printer_material_snapshots::ActiveModel {
        id: Set(id),
        tenant_id: Set(input.tenant_id.to_string()),
        printer_id: Set(input.printer_id),
        agent_id: Set(input.agent_id.to_string()),
        serial_number: Set(serial_number),
        ams_json: Set(serde_json::to_string(&merged.ams_units)
            .context("failed to serialize AMS material state")?),
        external_spools_json: Set(serde_json::to_string(&merged.external_spools)
            .context("failed to serialize external spool material state")?),
        active_tray_json: Set(active_tray_json),
        filament_switch_installed: Set(merged.filament_switch_installed),
        observed_at: Set(patch.observed_at),
        studio_cfg: Set(merged.cfg),
        studio_aux: Set(merged.aux),
        studio_stat: Set(merged.stat),
        updated_at: Set(now),
    };

    let model = if current.is_some() {
        model
            .update(connection)
            .await
            .context("failed to update material snapshot")?
    } else {
        model
            .insert(connection)
            .await
            .context("failed to insert material snapshot")?
    };
    snapshot_from_model(model).map(MaterialPatchOutcome::Changed)
}

fn snapshot_from_model(
    model: printer_material_snapshots::Model,
) -> RepositoryResult<MaterialSnapshot> {
    (|| -> anyhow::Result<MaterialSnapshot> {
        Ok(MaterialSnapshot {
            id: model.id,
            tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
            printer_id: model.printer_id,
            agent_id: AgentId::parse(&model.agent_id).map_err(anyhow::Error::from)?,
            serial_number: model.serial_number,
            ams_units: parse_array_json(&model.ams_json, "AMS material state")?,
            external_spools: parse_array_json(
                &model.external_spools_json,
                "external spool material state",
            )?,
            active_tray: model
                .active_tray_json
                .as_deref()
                .map(|json| parse_object_json(json, "active material tray"))
                .transpose()?,
            filament_switch_installed: model.filament_switch_installed,
            observed_at: model.observed_at,
            cfg: model.studio_cfg,
            aux: model.studio_aux,
            stat: model.studio_stat,
            updated_at: model.updated_at,
        })
    })()
    .context("failed to rehydrate material snapshot")
    .map_err(RepositoryError::from)
}
