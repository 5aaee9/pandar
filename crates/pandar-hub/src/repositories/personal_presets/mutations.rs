use anyhow::Context;
use pandar_core::{TenantId, created_at_now};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
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
    if let Err(error) = lock_owner(&tx, tenant_id, owner).await {
        tx.rollback()
            .await
            .context("failed to finish missing personal preset owner")?;
        return Err(error);
    }
    let existing = match find_by_name(&tx, tenant_id, owner, &input.name).await {
        Ok(value) => value,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset lookup error")?;
            return Err(error);
        }
    };
    if let Some(existing) = existing {
        let existing = match preset_from_model(existing) {
            Ok(preset) => preset,
            Err(error) => {
                tx.rollback()
                    .await
                    .context("failed to roll back personal preset conversion error")?;
                return Err(error);
            }
        };
        if replay_matches(&existing, &input) {
            tx.rollback()
                .await
                .context("failed to finish preset replay")?;
            return Ok(existing);
        }
        tx.rollback()
            .await
            .context("failed to finish duplicate preset create")?;
        return Err(RepositoryError::DuplicatePersonalPresetName);
    }
    let count = personal_presets::Entity::find()
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner))
        .count(&tx)
        .await;
    let count = match count {
        Ok(count) => count,
        Err(error) => {
            let error = anyhow::Error::new(error).context("failed to count owner personal presets");
            tx.rollback()
                .await
                .context("failed to roll back personal preset count error")?;
            return Err(error.into());
        }
    };
    if count >= validation::OWNER_PRESET_LIMIT {
        tx.rollback()
            .await
            .context("failed to finish preset quota rejection")?;
        return Err(RepositoryError::PersonalPresetLimitExceeded);
    }
    let updated_time = match advance_clock(&tx, tenant_id, owner).await {
        Ok(value) => value,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset clock error")?;
            return Err(error);
        }
    };
    let now = created_at_now();
    let id = uuid::Uuid::new_v4().to_string();
    let model = personal_presets::ActiveModel {
        id: Set(id),
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
    .insert(&tx)
    .await;
    let model = match model {
        Ok(model) => model,
        Err(error)
            if crate::db::is_unique_violation(
                &error,
                crate::db::UniqueConstraint::PersonalPresetName,
            ) =>
        {
            tx.rollback()
                .await
                .context("failed to finish concurrent duplicate preset create")?;
            return Err(RepositoryError::DuplicatePersonalPresetName);
        }
        Err(error) => {
            let error = anyhow::Error::new(error).context("failed to insert personal preset");
            tx.rollback()
                .await
                .context("failed to roll back personal preset insert error")?;
            return Err(error.into());
        }
    };
    let preset = match preset_from_model(model) {
        Ok(preset) => preset,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset conversion error")?;
            return Err(error);
        }
    };
    if let Err(error) = audit(
        &tx,
        tenant_id,
        actor,
        "personal_preset.create",
        &preset,
        options_json.len(),
    )
    .await
    {
        tx.rollback()
            .await
            .context("failed to roll back personal preset audit error")?;
        return Err(error);
    }
    tx.commit()
        .await
        .context("failed to commit personal preset create")?;
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
    if let Err(error) = lock_owner(&tx, tenant_id, owner).await {
        tx.rollback()
            .await
            .context("failed to finish missing personal preset owner")?;
        return Err(error);
    }
    let query = personal_presets::Entity::find_by_id(id)
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner));
    let locked = tx.lock_for_update(query).one(&tx).await;
    let locked = match locked {
        Ok(value) => value,
        Err(error) => {
            let error = anyhow::Error::new(error).context("failed to lock personal preset");
            tx.rollback()
                .await
                .context("failed to roll back personal preset lock error")?;
            return Err(error.into());
        }
    };
    let Some(model) = locked else {
        tx.rollback()
            .await
            .context("failed to finish missing preset replacement")?;
        return Err(RepositoryError::MissingPersonalPreset);
    };
    let named = match find_by_name(&tx, tenant_id, owner, &input.name).await {
        Ok(value) => value,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset lookup error")?;
            return Err(error);
        }
    };
    if let Some(named) = named
        && named.id != id
    {
        tx.rollback()
            .await
            .context("failed to finish duplicate preset replacement")?;
        return Err(RepositoryError::DuplicatePersonalPresetName);
    }
    let updated_time = match advance_clock(&tx, tenant_id, owner).await {
        Ok(value) => value,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset clock error")?;
            return Err(error);
        }
    };
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
    let updated = active.update(&tx).await;
    let updated = match updated {
        Ok(model) => model,
        Err(error)
            if crate::db::is_unique_violation(
                &error,
                crate::db::UniqueConstraint::PersonalPresetName,
            ) =>
        {
            tx.rollback()
                .await
                .context("failed to finish concurrent duplicate preset replacement")?;
            return Err(RepositoryError::DuplicatePersonalPresetName);
        }
        Err(error) => {
            let error = anyhow::Error::new(error).context("failed to replace personal preset");
            tx.rollback()
                .await
                .context("failed to roll back personal preset replace error")?;
            return Err(error.into());
        }
    };
    let preset = match preset_from_model(updated) {
        Ok(preset) => preset,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset conversion error")?;
            return Err(error);
        }
    };
    if let Err(error) = audit(
        &tx,
        tenant_id,
        actor,
        "personal_preset.update",
        &preset,
        options_json.len(),
    )
    .await
    {
        tx.rollback()
            .await
            .context("failed to roll back personal preset audit error")?;
        return Err(error);
    }
    tx.commit()
        .await
        .context("failed to commit personal preset replace")?;
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
    if let Err(error) = lock_owner(&tx, tenant_id, owner).await {
        tx.rollback()
            .await
            .context("failed to finish missing personal preset owner")?;
        return Err(error);
    }
    let query = personal_presets::Entity::find_by_id(id)
        .filter(personal_presets::Column::TenantId.eq(tenant_id.to_string()))
        .filter(personal_presets::Column::OwnerUserId.eq(owner));
    let locked = tx.lock_for_update(query).one(&tx).await;
    let locked = match locked {
        Ok(value) => value,
        Err(error) => {
            let error =
                anyhow::Error::new(error).context("failed to lock personal preset before delete");
            tx.rollback()
                .await
                .context("failed to roll back personal preset lock error")?;
            return Err(error.into());
        }
    };
    let Some(model) = locked else {
        tx.rollback()
            .await
            .context("failed to finish missing preset delete")?;
        return Ok(false);
    };
    let preset = match preset_from_model(model) {
        Ok(preset) => preset,
        Err(error) => {
            tx.rollback()
                .await
                .context("failed to roll back personal preset conversion error")?;
            return Err(error);
        }
    };
    if let Err(error) = advance_clock(&tx, tenant_id, owner).await {
        tx.rollback()
            .await
            .context("failed to roll back personal preset clock error")?;
        return Err(error);
    }
    let deleted = personal_presets::Entity::delete_by_id(id).exec(&tx).await;
    if let Err(error) = deleted {
        let error = anyhow::Error::new(error).context("failed to delete personal preset");
        tx.rollback()
            .await
            .context("failed to roll back personal preset delete error")?;
        return Err(error.into());
    }
    if let Err(error) = audit(&tx, tenant_id, actor, "personal_preset.delete", &preset, 0).await {
        tx.rollback()
            .await
            .context("failed to roll back personal preset audit error")?;
        return Err(error);
    }
    tx.commit()
        .await
        .context("failed to commit personal preset delete")?;
    Ok(true)
}
