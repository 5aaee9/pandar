use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseTransaction, EntityTrait,
    PaginatorTrait, QueryFilter,
};

use super::{
    CreatePersonalPreset, PersonalPreset, PersonalPresetRepository, mutation_support::*,
    preset_from_model, validation,
};
use crate::{
    db::ConnectionDialectExt,
    entities::personal_presets,
    repositories::{AuditActor, RepositoryError, RepositoryResult},
};

pub(super) async fn create(
    repository: &PersonalPresetRepository,
    tenant_id: TenantId,
    owner: &str,
    input: CreatePersonalPreset,
    actor: AuditActor,
) -> RepositoryResult<PersonalPreset> {
    validation::validate(&input)?;
    let options_json = encode_options(&input)?;
    let tx = repository
        .database
        .begin_write_transaction()
        .await
        .context("failed to begin personal preset create transaction")?;
    let preset = create_in_transaction(&tx, tenant_id, owner, input, options_json, actor).await?;
    tx.commit()
        .await
        .context("failed to commit personal preset create")?;
    Ok(preset)
}

async fn create_in_transaction(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    owner: &str,
    input: CreatePersonalPreset,
    options_json: String,
    actor: AuditActor,
) -> RepositoryResult<PersonalPreset> {
    lock_owner(tx, tenant_id, owner).await?;
    if let Some(existing) = find_by_name(tx, tenant_id, owner, &input.name).await? {
        let existing = preset_from_model(existing)?;
        return if replay_matches(&existing, &input) {
            Ok(existing)
        } else {
            Err(RepositoryError::DuplicatePersonalPresetName)
        };
    }
    let count = personal_presets::Entity::find()
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner))
        .count(tx)
        .await
        .context("failed to count owner personal presets")?;
    if count >= validation::OWNER_PRESET_LIMIT {
        return Err(RepositoryError::PersonalPresetLimitExceeded);
    }
    let updated_time = advance_clock(tx, tenant_id, owner).await?;
    let now = created_at_now();
    let model = personal_presets::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant_id.to_string()),
        owner_user_id: Set(owner.to_owned()),
        preset_type: Set(input.preset_type.as_str().to_owned()),
        name: Set(input.name),
        version: Set(input.version),
        base_id: Set(input.base_id),
        inherits: Set(input.inherits),
        filament_id: Set(input.filament_id),
        options_json: Set(options_json.clone()),
        updated_time: Set(updated_time),
        created_at: Set(now.clone()),
        updated_at: Set(now),
    }
    .insert(tx)
    .await
    .map_err(|error| {
        if crate::db::is_unique_violation(&error, crate::db::UniqueConstraint::PersonalPresetName) {
            RepositoryError::DuplicatePersonalPresetName
        } else {
            anyhow::Error::new(error)
                .context("failed to insert personal preset")
                .into()
        }
    })?;
    let preset = preset_from_model(model)?;
    audit(
        tx,
        tenant_id,
        actor,
        "personal_preset.create",
        &preset,
        options_json.len(),
    )
    .await?;
    Ok(preset)
}

pub(super) async fn replace(
    repository: &PersonalPresetRepository,
    tenant_id: TenantId,
    owner: &str,
    id: &str,
    input: CreatePersonalPreset,
    actor: AuditActor,
) -> RepositoryResult<PersonalPreset> {
    validation::validate(&input)?;
    let options_json = encode_options(&input)?;
    let tx = repository
        .database
        .begin_write_transaction()
        .await
        .context("failed to begin personal preset replace transaction")?;
    let preset =
        replace_in_transaction(&tx, tenant_id, owner, id, input, options_json, actor).await?;
    tx.commit()
        .await
        .context("failed to commit personal preset replace")?;
    Ok(preset)
}

async fn replace_in_transaction(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    owner: &str,
    id: &str,
    input: CreatePersonalPreset,
    options_json: String,
    actor: AuditActor,
) -> RepositoryResult<PersonalPreset> {
    lock_owner(tx, tenant_id, owner).await?;
    let query = personal_presets::Entity::find_by_id(id)
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner));
    let model = tx
        .lock_for_update(query)
        .one(tx)
        .await
        .context("failed to lock personal preset")?
        .ok_or(RepositoryError::MissingPersonalPreset)?;
    if find_by_name(tx, tenant_id, owner, &input.name)
        .await?
        .is_some_and(|named| named.id != id)
    {
        return Err(RepositoryError::DuplicatePersonalPresetName);
    }
    let updated_time = advance_clock(tx, tenant_id, owner).await?;
    let mut active: personal_presets::ActiveModel = model.into();
    active.preset_type = Set(input.preset_type.as_str().to_owned());
    active.name = Set(input.name);
    active.version = Set(input.version);
    active.base_id = Set(input.base_id);
    active.inherits = Set(input.inherits);
    active.filament_id = Set(input.filament_id);
    active.options_json = Set(options_json.clone());
    active.updated_time = Set(updated_time);
    active.updated_at = Set(created_at_now());
    let updated = active.update(tx).await.map_err(|error| {
        if crate::db::is_unique_violation(&error, crate::db::UniqueConstraint::PersonalPresetName) {
            RepositoryError::DuplicatePersonalPresetName
        } else {
            anyhow::Error::new(error)
                .context("failed to replace personal preset")
                .into()
        }
    })?;
    let preset = preset_from_model(updated)?;
    audit(
        tx,
        tenant_id,
        actor,
        "personal_preset.update",
        &preset,
        options_json.len(),
    )
    .await?;
    Ok(preset)
}

pub(super) async fn delete(
    repository: &PersonalPresetRepository,
    tenant_id: TenantId,
    owner: &str,
    id: &str,
    actor: AuditActor,
) -> RepositoryResult<bool> {
    let tx = repository
        .database
        .begin_write_transaction()
        .await
        .context("failed to begin personal preset delete transaction")?;
    let deleted = delete_in_transaction(&tx, tenant_id, owner, id, actor).await?;
    tx.commit()
        .await
        .context("failed to commit personal preset delete")?;
    Ok(deleted)
}

async fn delete_in_transaction(
    tx: &DatabaseTransaction,
    tenant_id: TenantId,
    owner: &str,
    id: &str,
    actor: AuditActor,
) -> RepositoryResult<bool> {
    lock_owner(tx, tenant_id, owner).await?;
    let query = personal_presets::Entity::find_by_id(id)
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner));
    let Some(model) = tx
        .lock_for_update(query)
        .one(tx)
        .await
        .context("failed to lock personal preset before delete")?
    else {
        return Ok(false);
    };
    let preset = preset_from_model(model)?;
    advance_clock(tx, tenant_id, owner).await?;
    personal_presets::Entity::delete_by_id(id)
        .exec(tx)
        .await
        .context("failed to delete personal preset")?;
    audit(tx, tenant_id, actor, "personal_preset.delete", &preset, 0).await?;
    Ok(true)
}
