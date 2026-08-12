use anyhow::Context;
use pandar_core::TenantId;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serde::Serialize;

use super::{CreatePersonalPreset, PersonalPreset};
use crate::{
    db::ConnectionDialectExt,
    entities::{personal_preset_clocks, personal_presets, users},
    repositories::{
        AuditActor, RepositoryError, RepositoryResult,
        audit::{audit_metadata, insert_audit_event_tx, record_audit_event},
    },
};

#[derive(Serialize)]
struct PresetAudit<'a> {
    preset_type: &'a str,
    byte_size: usize,
}

pub(super) async fn lock_owner(
    tx: &sea_orm::DatabaseTransaction,
    tenant_id: TenantId,
    owner: &str,
) -> RepositoryResult<()> {
    let query =
        users::Entity::find_by_id(owner).filter(users::Column::TenantId.eq(tenant_id.to_string()));
    tx.lock_for_update(query)
        .one(tx)
        .await
        .context("failed to lock personal preset owner")?
        .ok_or(RepositoryError::MissingUser)?;
    Ok(())
}

pub(super) async fn find_by_name(
    tx: &sea_orm::DatabaseTransaction,
    tenant_id: TenantId,
    owner: &str,
    name: &str,
) -> RepositoryResult<Option<personal_presets::Model>> {
    personal_presets::Entity::find()
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner))
        .filter(personal_presets::Column::Name.eq(name))
        .one(tx)
        .await
        .context("failed to find personal preset by name")
        .map_err(Into::into)
}

pub(super) async fn advance_clock(
    tx: &sea_orm::DatabaseTransaction,
    tenant_id: TenantId,
    owner: &str,
) -> RepositoryResult<i64> {
    let key = (tenant_id.to_string(), owner.to_owned());
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let model = tx
        .lock_for_update(personal_preset_clocks::Entity::find_by_id(key.clone()))
        .one(tx)
        .await
        .context("failed to lock personal preset clock")?;
    let Some(model) = model else {
        personal_preset_clocks::ActiveModel {
            tenant_id: Set(key.0),
            owner_user_id: Set(key.1),
            last_updated_time: Set(now),
        }
        .insert(tx)
        .await
        .context("failed to create personal preset clock")?;
        return Ok(now);
    };
    if model.last_updated_time == i64::MAX {
        return Err(RepositoryError::PersonalPresetClockExhausted);
    }
    let next = now.max(model.last_updated_time + 1);
    let mut active: personal_preset_clocks::ActiveModel = model.into();
    active.last_updated_time = Set(next);
    active
        .update(tx)
        .await
        .context("failed to advance personal preset clock")?;
    Ok(next)
}

pub(super) fn replay_matches(existing: &PersonalPreset, input: &CreatePersonalPreset) -> bool {
    existing.preset_type == input.preset_type
        && existing.version == input.version
        && existing.base_id == input.base_id
        && existing.inherits == input.inherits
        && existing.filament_id == input.filament_id
        && existing.options == input.options
}

pub(super) fn encode_options(input: &CreatePersonalPreset) -> RepositoryResult<String> {
    serde_json::to_string(&input.options)
        .context("failed to encode personal preset options")
        .map_err(Into::into)
}

pub(super) async fn audit(
    tx: &sea_orm::DatabaseTransaction,
    tenant_id: TenantId,
    actor: AuditActor,
    action: &str,
    preset: &PersonalPreset,
    byte_size: usize,
) -> RepositoryResult<()> {
    insert_audit_event_tx(
        tx,
        &record_audit_event(
            tenant_id,
            actor,
            action,
            "personal_preset",
            Some(preset.id.clone()),
            audit_metadata(PresetAudit {
                preset_type: preset.preset_type.as_str(),
                byte_size,
            }),
        ),
    )
    .await
}
