use super::*;

#[tokio::test]
async fn personal_preset_inner_failure_rolls_back_once_and_preserves_the_cause() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database.clone());
    let tenant = tenants.create("preset-rollback", "Presets").await.unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    let Database::Sqlite(pool) = &database else {
        panic!("expected SQLite database");
    };
    sqlx::query(
        "CREATE TRIGGER fail_personal_preset_audit \
         BEFORE INSERT ON audit_events \
         WHEN NEW.action = 'personal_preset.create' \
         BEGIN SELECT RAISE(ABORT, 'forced audit failure'); END",
    )
    .execute(pool)
    .await
    .unwrap();

    let error = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Rolled Back"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(error.contains("failed to insert audit event"), "{error}");
    assert!(error.contains("forced audit failure"), "{error}");
    assert!(
        presets
            .list_metadata(tenant.id, &user.id)
            .await
            .unwrap()
            .is_empty()
    );

    sqlx::query("DROP TRIGGER fail_personal_preset_audit")
        .execute(pool)
        .await
        .unwrap();
    presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input(PersonalPresetType::Print, "Rolled Back"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
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
