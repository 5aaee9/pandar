use sea_orm::{ActiveValue::Set, EntityTrait};

use super::{personal_presets::input, sqlite_database};
use crate::{
    entities::personal_presets,
    repositories::{
        AuditActor, AuthRepository, PersonalPresetRepository, PersonalPresetType, RepositoryError,
        TenantRepository, UserRole,
    },
};

#[tokio::test]
async fn personal_preset_quota_rejects_the_next_owner_create() {
    let database = sqlite_database().await;
    let tenants = TenantRepository::new(database.clone());
    let auth = AuthRepository::new(database.clone());
    let presets = PersonalPresetRepository::new(database.clone());
    let tenant = tenants.create("preset-quota", "Presets").await.unwrap();
    let user = auth
        .create_user(tenant.id, "owner@test", "Owner", UserRole::Operator)
        .await
        .unwrap();
    let now = pandar_core::created_at_now();
    let models = (0..1_000).map(|index| personal_presets::ActiveModel {
        id: Set(uuid::Uuid::new_v4().to_string()),
        tenant_id: Set(tenant.id.to_string()),
        owner_user_id: Set(user.id.clone()),
        preset_type: Set("print".to_owned()),
        name: Set(format!("Preset {index}")),
        version: Set("2.8.1.55".to_owned()),
        base_id: Set(String::new()),
        inherits: Set(None),
        filament_id: Set(None),
        options_json: Set("{}".to_owned()),
        updated_time: Set(i64::from(index)),
        created_at: Set(now.clone()),
        updated_at: Set(now.clone()),
    });
    personal_presets::Entity::insert_many(models)
        .exec_without_returning(&database.sea_orm_connection())
        .await
        .unwrap();
    assert!(matches!(
        presets
            .create_with_audit(
                tenant.id,
                &user.id,
                input(PersonalPresetType::Print, "One Too Many"),
                AuditActor::user(&user.id),
            )
            .await
            .unwrap_err(),
        RepositoryError::PersonalPresetLimitExceeded
    ));
}
