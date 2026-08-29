use std::collections::BTreeMap;

use super::postgres_database;
use crate::repositories::{
    AuditActor, AuthRepository, CreatePersonalPreset, PersonalPresetRepository, PersonalPresetType,
    TenantRepository, UserRole,
};

fn input(name: &str) -> CreatePersonalPreset {
    CreatePersonalPreset {
        preset_type: PersonalPresetType::Print,
        name: name.to_owned(),
        version: "2.8.1.55".to_owned(),
        base_id: String::new(),
        inherits: None,
        filament_id: None,
        options: BTreeMap::from([("layer_height".to_owned(), "0.2".to_owned())]),
    }
}

#[tokio::test]
async fn postgres_personal_preset_clock_serializes_concurrent_first_writes_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let repository = PersonalPresetRepository::new(database);
    let tenant = tenants
        .create("pg-presets", "Postgres Presets")
        .await
        .unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();

    let (first, second) = tokio::join!(
        repository.create_with_audit(
            tenant.id,
            &user.id,
            input("First"),
            AuditActor::user(&user.id)
        ),
        repository.create_with_audit(
            tenant.id,
            &user.id,
            input("Second"),
            AuditActor::user(&user.id)
        ),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_ne!(first.updated_time, second.updated_time);
    assert_eq!(
        repository
            .list_metadata(tenant.id, &user.id)
            .await
            .unwrap()
            .len(),
        2
    );
    repository
        .delete_with_audit(tenant.id, &user.id, &first.id, AuditActor::user(&user.id))
        .await
        .unwrap();
    repository
        .delete_with_audit(tenant.id, &user.id, &second.id, AuditActor::user(&user.id))
        .await
        .unwrap();
    let recreated = repository
        .create_with_audit(
            tenant.id,
            &user.id,
            input("Recreated"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
    assert!(recreated.updated_time > first.updated_time.max(second.updated_time));
}

#[tokio::test]
async fn postgres_personal_preset_inner_failure_rolls_back_and_preserves_cause_when_configured() {
    let Some(database) = postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database.clone());
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let tenant = tenants
        .create(&format!("pg-preset-rollback-{suffix}"), "Presets")
        .await
        .unwrap();
    let user = auth
        .create_user(
            tenant.id,
            &format!("owner-{suffix}@test"),
            "Owner",
            UserRole::Operator,
        )
        .await
        .unwrap();
    let crate::db::Database::Postgres(pool) = &database else {
        unreachable!();
    };
    let function = format!("fail_personal_preset_audit_{suffix}");
    let trigger = format!("fail_personal_preset_audit_trigger_{suffix}");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION {function}() RETURNS trigger AS $$ \
         BEGIN \
           IF NEW.action = 'personal_preset.create' AND NEW.tenant_id = '{}' THEN \
             RAISE EXCEPTION 'forced audit failure'; \
           END IF; \
           RETURN NEW; \
         END; \
         $$ LANGUAGE plpgsql",
        tenant.id
    )))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER {trigger} BEFORE INSERT ON audit_events \
         FOR EACH ROW EXECUTE FUNCTION {function}()"
    )))
    .execute(pool)
    .await
    .unwrap();

    let error = presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input("Rolled Back"),
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

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {trigger} ON audit_events"
    )))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP FUNCTION {function}()")))
        .execute(pool)
        .await
        .unwrap();
    presets
        .create_with_audit(
            tenant.id,
            &user.id,
            input("Rolled Back"),
            AuditActor::user(&user.id),
        )
        .await
        .unwrap();
}
