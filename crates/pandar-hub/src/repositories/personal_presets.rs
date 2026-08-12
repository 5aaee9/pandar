mod mutation_support;
mod mutations;
mod validation;

use std::collections::BTreeMap;

use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::{
    db::Database,
    entities::personal_presets,
    repositories::{AuditActor, RepositoryError, RepositoryResult},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonalPresetType {
    Print,
    Filament,
    Printer,
}

impl PersonalPresetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Filament => "filament",
            Self::Printer => "printer",
        }
    }

    fn parse(value: &str) -> RepositoryResult<Self> {
        match value {
            "print" => Ok(Self::Print),
            "filament" => Ok(Self::Filament),
            "printer" => Ok(Self::Printer),
            other => Err(RepositoryError::InvalidPersistedPersonalPreset(
                anyhow::anyhow!("unknown preset type {other}"),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonalPresetMetadata {
    pub id: String,
    pub preset_type: PersonalPresetType,
    pub name: String,
    pub version: String,
    pub base_id: String,
    pub inherits: Option<String>,
    pub filament_id: Option<String>,
    pub updated_time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonalPreset {
    pub id: String,
    pub tenant_id: TenantId,
    pub owner_user_id: String,
    pub preset_type: PersonalPresetType,
    pub name: String,
    pub version: String,
    pub base_id: String,
    pub inherits: Option<String>,
    pub filament_id: Option<String>,
    pub options: BTreeMap<String, String>,
    pub updated_time: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CreatePersonalPreset {
    pub preset_type: PersonalPresetType,
    pub name: String,
    pub version: String,
    pub base_id: String,
    pub inherits: Option<String>,
    pub filament_id: Option<String>,
    pub options: BTreeMap<String, String>,
}

pub type ReplacePersonalPreset = CreatePersonalPreset;

#[derive(Debug, Clone)]
pub struct PersonalPresetRepository {
    database: Database,
}

impl PersonalPresetRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub async fn list_metadata(
        &self,
        tenant_id: TenantId,
        owner_user_id: &str,
    ) -> RepositoryResult<Vec<PersonalPresetMetadata>> {
        personal_presets::Entity::find()
            .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(personal_presets::Column::OwnerUserId.eq(owner_user_id))
            .order_by_asc(personal_presets::Column::UpdatedTime)
            .order_by_asc(personal_presets::Column::Id)
            .all(&self.database.sea_orm_connection())
            .await
            .context("failed to list personal presets")?
            .into_iter()
            .map(metadata_from_model)
            .collect()
    }

    pub async fn get(
        &self,
        tenant_id: TenantId,
        owner_user_id: &str,
        setting_id: &str,
    ) -> RepositoryResult<Option<PersonalPreset>> {
        personal_presets::Entity::find_by_id(setting_id)
            .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
            .filter(personal_presets::Column::OwnerUserId.eq(owner_user_id))
            .one(&self.database.sea_orm_connection())
            .await
            .context("failed to get personal preset")?
            .map(preset_from_model)
            .transpose()
    }

    pub async fn create_with_audit(
        &self,
        tenant_id: TenantId,
        owner_user_id: &str,
        input: CreatePersonalPreset,
        actor: AuditActor,
    ) -> RepositoryResult<PersonalPreset> {
        mutations::create(self, tenant_id, owner_user_id, input, actor).await
    }

    pub async fn replace_with_audit(
        &self,
        tenant_id: TenantId,
        owner_user_id: &str,
        setting_id: &str,
        input: ReplacePersonalPreset,
        actor: AuditActor,
    ) -> RepositoryResult<PersonalPreset> {
        mutations::replace(self, tenant_id, owner_user_id, setting_id, input, actor).await
    }

    pub async fn delete_with_audit(
        &self,
        tenant_id: TenantId,
        owner_user_id: &str,
        setting_id: &str,
        actor: AuditActor,
    ) -> RepositoryResult<bool> {
        mutations::delete(self, tenant_id, owner_user_id, setting_id, actor).await
    }
}

fn metadata_from_model(model: personal_presets::Model) -> RepositoryResult<PersonalPresetMetadata> {
    Ok(PersonalPresetMetadata {
        id: model.id,
        preset_type: PersonalPresetType::parse(&model.preset_type)?,
        name: model.name,
        version: model.version,
        base_id: model.base_id,
        inherits: model.inherits,
        filament_id: model.filament_id,
        updated_time: model.updated_time,
    })
}

fn preset_from_model(model: personal_presets::Model) -> RepositoryResult<PersonalPreset> {
    let options = serde_json::from_str(&model.options_json)
        .context("failed to decode persisted personal preset options")
        .map_err(RepositoryError::InvalidPersistedPersonalPreset)?;
    Ok(PersonalPreset {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        owner_user_id: model.owner_user_id,
        preset_type: PersonalPresetType::parse(&model.preset_type)?,
        name: model.name,
        version: model.version,
        base_id: model.base_id,
        inherits: model.inherits,
        filament_id: model.filament_id,
        options,
        updated_time: model.updated_time,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}
