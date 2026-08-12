use std::collections::BTreeMap;

use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use super::sqlite_database;
use crate::{
    entities::personal_presets,
    repositories::{
        AuditActor, AuditEventRepository, AuthRepository, CreatePersonalPreset,
        PersonalPresetRepository, PersonalPresetType, RepositoryError, TenantRepository, UserRole,
    },
};

pub(super) fn input(kind: PersonalPresetType, name: &str) -> CreatePersonalPreset {
    CreatePersonalPreset {
        preset_type: kind,
        name: name.to_owned(),
        version: "2.8.1.55".to_owned(),
        base_id: String::new(),
        inherits: None,
        filament_id: matches!(kind, PersonalPresetType::Filament).then(|| "P123".to_owned()),
        options: BTreeMap::from([("layer_height".to_owned(), "0.2".to_owned())]),
    }
}

#[tokio::test]
async fn personal_preset_repository_round_trips_replays_updates_and_deletes() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database.clone());
    let audit = AuditEventRepository::new(database);
    let tenant = tenants.create("preset-roundtrip", "Presets").await.unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();

    let created = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Fine"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    let replay = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Fine"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    assert_eq!(replay.id, created.id);
    assert_eq!(replay.updated_time, created.updated_time);
    assert_eq!(
        presets.get(tenant.id, &user.id, &created.id).await.unwrap(),
        Some(created.clone())
    );
    assert_eq!(
        presets
            .list_metadata(tenant.id, &user.id)
            .await
            .unwrap()
            .len(),
        1
    );

    let mut replacement = input(PersonalPresetType::Printer, "Machine");
    replacement
        .options
        .insert("printer_model".into(), "X1C".into());
    let replaced = presets
        .replace_with_audit(
            tenant.id,
            &user.id,
            &created.id,
            replacement,
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    assert!(replaced.updated_time > created.updated_time);
    assert_eq!(replaced.preset_type, PersonalPresetType::Printer);
    assert!(
        presets
            .delete_with_audit(tenant.id, &user.id, &created.id, AuditActor::user(&user.id))
            .await
            .unwrap()
    );
    assert!(
        !presets
            .delete_with_audit(tenant.id, &user.id, &created.id, AuditActor::user(&user.id))
            .await
            .unwrap()
    );

    let events = audit.list_for_tenant(tenant.id).await.unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.action == "personal_preset.create")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.action == "personal_preset.update")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.action == "personal_preset.delete")
            .count(),
        1
    );
    for event in events
        .iter()
        .filter(|event| event.target_type == "personal_preset")
    {
        assert!(!event.metadata_json.contains("layer_height"));
        assert!(!event.metadata_json.contains("printer_model"));
    }
}

#[tokio::test]
async fn personal_presets_are_owner_scoped_and_names_are_global_across_types() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database);
    let tenant = tenants.create("preset-owner", "Presets").await.unwrap();
    let owner = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    let other = auth
        .create_user(tenant.id, "other@test", "Other", UserRole::Operator)
        .await
        .unwrap();
    let created = presets
        .create_with_audit(
            tenant.id,
            &owner.id,
            input(PersonalPresetType::Print, "Shared"),
            AuditActor::user(&owner.id),
        )
        .await
        .unwrap();

    assert!(
        presets
            .get(tenant.id, &other.id, &created.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !presets
            .delete_with_audit(
                tenant.id,
                &other.id,
                &created.id,
                AuditActor::user(&other.id)
            )
            .await
            .unwrap()
    );
    let error = presets
        .create_with_audit(
            tenant.id,
            &owner.id,
            input(PersonalPresetType::Filament, "Shared"),
            AuditActor::user(&owner.id),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RepositoryError::DuplicatePersonalPresetName
    ));
    let other_created = presets
        .create_with_audit(
            tenant.id,
            &other.id,
            input(PersonalPresetType::Print, "Shared"),
            AuditActor::user(&other.id),
        )
        .await
        .unwrap();
    assert_ne!(created.id, other_created.id);

    let second = presets
        .create_with_audit(
            tenant.id,
            &owner.id,
            input(PersonalPresetType::Print, "Second"),
            AuditActor::user(&owner.id),
        )
        .await
        .unwrap();
    let error = presets
        .replace_with_audit(
            tenant.id,
            &owner.id,
            &second.id,
            input(PersonalPresetType::Printer, "Shared"),
            AuditActor::user(&owner.id),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RepositoryError::DuplicatePersonalPresetName
    ));
}

#[tokio::test]
async fn personal_presets_are_tenant_scoped_even_for_guessed_ids() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database);
    let first_tenant = tenants.create("preset-first", "First").await.unwrap();
    let second_tenant = tenants.create("preset-second", "Second").await.unwrap();
    let first = auth
        .create_user(first_tenant.id, "first@test", "First", UserRole::Operator)
        .await
        .unwrap();
    let second = auth
        .create_user(
            second_tenant.id,
            "second@test",
            "Second",
            UserRole::Operator,
        )
        .await
        .unwrap();
    let created = presets
        .create_with_audit(
            first_tenant.id,
            &first.id,
            input(PersonalPresetType::Print, "Private"),
            AuditActor::user(&first.id),
        )
        .await
        .unwrap();
    assert!(
        presets
            .get(second_tenant.id, &second.id, &created.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn durable_owner_clock_survives_deleting_the_final_preset() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database);
    let tenant = tenants.create("preset-clock", "Presets").await.unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    let first = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "First"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    presets
        .delete_with_audit(tenant.id, &user.id, &first.id, AuditActor::user(&user.id))
        .await
        .unwrap();
    let second = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Second"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    assert!(second.updated_time > first.updated_time);
}

#[tokio::test]
async fn deleting_a_user_cascades_personal_presets_and_owner_clock() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database);
    let tenant = tenants.create("preset-cascade", "Presets").await.unwrap();
    let admin = auth
        .create_user(tenant.id, "admin@test", "Admin", UserRole::TenantAdmin)
        .await
        .unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Cascade"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    auth.remove_user_with_audit(tenant.id, &user.id, AuditActor::user(admin.id))
        .await
        .unwrap();
    assert!(
        presets
            .list_metadata(tenant.id, &user.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn invalid_and_malformed_personal_presets_preserve_stable_errors_and_causes() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database.clone());
    let tenant = tenants.create("preset-invalid", "Presets").await.unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    let mut invalid = input(PersonalPresetType::Print, "");
    assert!(matches!(
        presets
            .create_with_audit(
                tenant.id,
                &user.id,
                invalid.clone(),
                AuditActor::user(&user.id)
            )
            .await
            .unwrap_err(),
        RepositoryError::InvalidPersonalPreset
    ));
    invalid.name = "Large".into();
    invalid
        .options
        .insert("gcode".into(), "x".repeat(64 * 1024 + 1));
    assert!(matches!(
        presets
            .create_with_audit(tenant.id, &user.id, invalid, AuditActor::user(&user.id))
            .await
            .unwrap_err(),
        RepositoryError::InvalidPersonalPreset
    ));

    let created = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Broken"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    personal_presets::Entity::update_many()
        .filter(personal_presets::Column::Id.eq(&created.id))
        .set(personal_presets::ActiveModel {
            options_json: Set("{".into()),
            ..Default::default()
        })
        .exec(&database.sea_orm_connection())
        .await
        .unwrap();
    let error = presets
        .get(tenant.id, &user.id, &created.id)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RepositoryError::InvalidPersistedPersonalPreset(_)
    ));
    assert!(format!("{error:#}").contains("failed to decode persisted personal preset options"));

    let mut invalid_filament_metadata = input(PersonalPresetType::Print, "Bad Metadata");
    invalid_filament_metadata.filament_id = Some("P123".to_owned());
    assert!(matches!(
        presets
            .create_with_audit(
                tenant.id,
                &user.id,
                invalid_filament_metadata,
                AuditActor::user(&user.id),
            )
            .await
            .unwrap_err(),
        RepositoryError::InvalidPersonalPreset
    ));
}

#[tokio::test]
async fn personal_preset_map_size_limit_uses_studio_key_value_bytes() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database);
    let tenant = tenants.create("preset-size", "Presets").await.unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    let mut oversized = input(PersonalPresetType::Print, "Oversized");
    oversized.options = (0..6)
        .map(|index| (format!("option_{index}"), "x".repeat(60 * 1024)))
        .collect();
    assert!(matches!(
        presets
            .create_with_audit(tenant.id, &user.id, oversized, AuditActor::user(&user.id),)
            .await
            .unwrap_err(),
        RepositoryError::PersonalPresetTooLarge
    ));
}
