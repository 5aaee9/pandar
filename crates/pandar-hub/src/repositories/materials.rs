use anyhow::Context;
use pandar_core::{AgentId, TenantId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
use serde_json::Value;

use crate::{
    db::Database,
    entities::{printer_material_snapshots, printers},
    repositories::{RepositoryError, RepositoryResult},
};

mod merge;
mod patch;

use merge::merge_snapshot;
use patch::{is_older, parse_array_json, parse_object_json, parse_patch};

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
    pub ams_units: Value,
    pub external_spools: Value,
    pub active_tray: Option<Value>,
    pub observed_at: String,
    pub updated_at: String,
}

impl MaterialSnapshot {
    #[cfg(test)]
    pub(crate) fn persisted_json(&self) -> String {
        serde_json::json!({
            "ams_units": self.ams_units,
            "external_spools": self.external_spools,
            "active_tray": self.active_tray
        })
        .to_string()
    }
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

    pub async fn upsert_from_patch(
        &self,
        input: MaterialPatchInput,
    ) -> RepositoryResult<Option<MaterialSnapshot>> {
        let connection = self.database.sea_orm_connection();
        upsert_from_patch_in_connection(&connection, input).await
    }
}

pub(crate) async fn upsert_from_patch_in_connection<C>(
    connection: &C,
    input: MaterialPatchInput,
) -> RepositoryResult<Option<MaterialSnapshot>>
where
    C: sea_orm::ConnectionTrait,
{
    let Some(patch) = parse_patch(&input.printer_materials_json) else {
        return Ok(None);
    };

    let Some(printer) = printers::Entity::find_by_id(&input.printer_id)
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(input.agent_id.to_string()))
        .one(connection)
        .await
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
        return Ok(None);
    }

    let merged = merge_snapshot(current.as_ref(), &patch)?;
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
        observed_at: Set(patch.observed_at),
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
    snapshot_from_model(model).map(Some)
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
            ams_units: Value::Array(parse_array_json(&model.ams_json, "AMS material state")?),
            external_spools: Value::Array(parse_array_json(
                &model.external_spools_json,
                "external spool material state",
            )?),
            active_tray: model
                .active_tray_json
                .as_deref()
                .map(|json| parse_object_json(json, "active material tray"))
                .transpose()?,
            observed_at: model.observed_at,
            updated_at: model.updated_at,
        })
    })()
    .context("failed to rehydrate material snapshot")
    .map_err(RepositoryError::from)
}
