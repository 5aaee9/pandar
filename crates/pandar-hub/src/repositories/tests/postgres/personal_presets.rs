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
