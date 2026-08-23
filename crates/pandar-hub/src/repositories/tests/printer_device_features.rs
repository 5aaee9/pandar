use pandar_core::{
    BambuDeviceFeatures, BambuNozzleDevice, BambuNozzleInfo, BambuNozzleSystem,
    PrinterNozzleTemperature, StudioFiniteF64, TenantId,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use sqlx::migrate::Migrator;

use super::*;
use crate::{db::Database, entities::printers, repositories::test_helpers::insert_printer_fixture};

const FULL_BITS: u64 = 0x8000_0041_0000_0020;
const FEATURE_ONLY_BITS: u64 = 0x8000_0041_0000_0021;
const PREVIOUS_MIGRATION: i64 = 20260710000000;
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("migrations/sqlite");
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("migrations/postgres");
const SQLITE_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/sqlite/20260711000000_bambu_device_features.sql"
));
const POSTGRES_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/postgres/20260711000000_bambu_device_features.sql"
));

#[test]
fn printer_device_features_migrations_are_byte_identical() {
    assert_eq!(SQLITE_MIGRATION.as_bytes(), POSTGRES_MIGRATION.as_bytes());
}

#[tokio::test]
async fn printer_device_features_sqlite_matrix() {
    exercise_printer_device_features(sqlite_database().await).await;
}

#[tokio::test]
async fn printer_device_features_sqlite_migrates_legacy_rows_to_null() {
    let config = crate::db::DatabaseConfig::from_url("sqlite::memory:").unwrap();
    let database = Database::connect(&config).await.unwrap();

    exercise_legacy_device_features_migration(database).await;
}

pub(super) async fn exercise_legacy_device_features_migration(database: Database) {
    match &database {
        Database::Sqlite(pool) => SQLITE_MIGRATOR
            .run_to(PREVIOUS_MIGRATION, pool)
            .await
            .unwrap(),
        Database::Postgres(pool) => POSTGRES_MIGRATOR
            .run_to(PREVIOUS_MIGRATION, pool)
            .await
            .unwrap(),
    }

    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let tenant = tenants
        .create(
            &format!("legacy-device-features-{}", uuid::Uuid::new_v4()),
            "Legacy Device Features",
        )
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "legacy-agent").await.unwrap();
    let printer_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();

    match &database {
        Database::Sqlite(pool) => SQLITE_MIGRATOR.run(pool).await.unwrap(),
        Database::Postgres(pool) => POSTGRES_MIGRATOR.run(pool).await.unwrap(),
    }

    let columns: (Option<String>, Option<String>, Option<String>, Option<String>) = match &database {
        Database::Sqlite(pool) => sqlx::query_as(
            "SELECT bambu_fun_bits, bambu_fun_session_id, bambu_fun2_bits, bambu_fun2_session_id FROM printers WHERE id = ?1",
        )
        .bind(&printer_id)
        .fetch_one(pool)
        .await
        .unwrap(),
        Database::Postgres(pool) => sqlx::query_as(
            "SELECT bambu_fun_bits, bambu_fun_session_id, bambu_fun2_bits, bambu_fun2_session_id FROM printers WHERE id = $1",
        )
        .bind(&printer_id)
        .fetch_one(pool)
        .await
        .unwrap(),
    };
    assert_eq!(columns, (None, None, None, None));
}

pub(super) async fn exercise_printer_device_features(database: Database) {
    let tenants = TenantRepository::new(database.clone());
    let agents = AgentRepository::new(database.clone());
    let printers = PrinterRepository::new(database.clone());
    let tenant = tenants
        .create(
            &format!("device-features-{}", uuid::Uuid::new_v4()),
            "Device Features",
        )
        .await
        .unwrap();
    let other_tenant = tenants
        .create(
            &format!("other-device-features-{}", uuid::Uuid::new_v4()),
            "Other Device Features",
        )
        .await
        .unwrap();
    let agent = agents.create(tenant.id, "agent").await.unwrap();
    let other_agent = agents.create(tenant.id, "other-agent").await.unwrap();

    let legacy_id = insert_printer_fixture(&database, tenant.id, agent.id)
        .await
        .unwrap();
    let legacy = printers::Entity::find_by_id(legacy_id)
        .one(&database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(legacy.bambu_fun_bits, None);
    assert_eq!(legacy.bambu_fun_session_id, None);

    let session_one = "device-feature-session-one";
    claim_session(&agents, tenant.id, agent.id, session_one).await;
    let mut initial_snapshot = rich_snapshot("FEATURE-SERIAL", "printing");
    initial_snapshot.nozzle_system = Some(nozzle_system(16));
    let created = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_one,
            initial_snapshot,
            Some(BambuDeviceFeatures::from_bits(FULL_BITS)),
        )
        .await
        .unwrap();
    assert_eq!(
        created.bambu_device_features,
        Some(BambuDeviceFeatures::from_bits(FULL_BITS))
    );
    assert_eq!(
        created.bambu_device_features_session_id.as_deref(),
        Some(session_one)
    );
    assert_eq!(created.bambu_nozzle_system, Some(nozzle_system(16)));
    assert_eq!(
        created.bambu_nozzle_system_session_id.as_deref(),
        Some(session_one)
    );
    let stored = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    assert_eq!(stored.bambu_fun_bits.as_deref(), Some("8000004100000020"));
    assert_eq!(stored.bambu_fun_session_id.as_deref(), Some(session_one));
    assert_eq!(
        stored.bambu_nozzle_system_session_id.as_deref(),
        Some(session_one)
    );
    assert_eq!(
        printers
            .update_secondary_device_features_if_current(
                tenant.id,
                agent.id,
                session_one,
                "FEATURE-SERIAL",
                Some(BambuDeviceFeatures::from_bits(0x8000_0000_0000_0021)),
            )
            .await
            .unwrap(),
        DeviceFeatureUpdateOutcome::Updated
    );
    let secondary = printers
        .get_for_tenant(tenant.id, &created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        secondary.bambu_device_features2,
        Some(BambuDeviceFeatures::from_bits(0x8000_0000_0000_0021))
    );
    assert_eq!(
        secondary.bambu_device_features2_session_id.as_deref(),
        Some(session_one)
    );
    assert_eq!(
        printers
            .update_secondary_device_features_if_current(
                tenant.id,
                agent.id,
                session_one,
                "FEATURE-SERIAL",
                Some(BambuDeviceFeatures::from_bits(0x8000_0000_0000_0021)),
            )
            .await
            .unwrap(),
        DeviceFeatureUpdateOutcome::Unchanged
    );

    let session_two = "device-feature-session-two";
    claim_session(&agents, tenant.id, agent.id, session_two).await;
    let absent = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            rich_snapshot("FEATURE-SERIAL", "idle"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        absent.bambu_device_features,
        Some(BambuDeviceFeatures::from_bits(FULL_BITS))
    );
    assert_eq!(
        absent.bambu_device_features_session_id.as_deref(),
        Some(session_one)
    );
    assert_eq!(absent.bambu_nozzle_system, Some(nozzle_system(16)));
    assert_eq!(
        absent.bambu_nozzle_system_session_id.as_deref(),
        Some(session_one)
    );
    let stored = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    assert_eq!(stored.bambu_fun_bits.as_deref(), Some("8000004100000020"));
    assert_eq!(stored.bambu_fun_session_id.as_deref(), Some(session_one));
    assert_eq!(
        stored.bambu_nozzle_system_session_id.as_deref(),
        Some(session_one)
    );

    let zero = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            rich_snapshot("FEATURE-SERIAL", "idle"),
            Some(BambuDeviceFeatures::default()),
        )
        .await
        .unwrap();
    assert_eq!(
        zero.bambu_device_features,
        Some(BambuDeviceFeatures::default())
    );
    assert_eq!(
        zero.bambu_device_features_session_id.as_deref(),
        Some(session_two)
    );
    let stored = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    assert_eq!(stored.bambu_fun_bits.as_deref(), Some("0"));
    assert_eq!(stored.bambu_fun_session_id.as_deref(), Some(session_two));

    let mut nozzle_update = rich_snapshot("FEATURE-SERIAL", "idle");
    nozzle_update.nozzle_system = Some(nozzle_system(17));
    let nozzle_updated = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            nozzle_update,
            None,
        )
        .await
        .unwrap();
    assert_eq!(nozzle_updated.bambu_nozzle_system, Some(nozzle_system(17)));
    assert_eq!(
        nozzle_updated.bambu_nozzle_system_session_id.as_deref(),
        Some(session_two)
    );

    let high_bit = printers
        .upsert_snapshot_with_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            rich_snapshot("FEATURE-SERIAL", "idle"),
            Some(BambuDeviceFeatures::from_bits(FULL_BITS)),
        )
        .await
        .unwrap();
    assert_eq!(
        high_bit.bambu_device_features,
        Some(BambuDeviceFeatures::from_bits(FULL_BITS))
    );
    assert_eq!(high_bit.bambu_device_features.unwrap().bits(), FULL_BITS);
    let stored = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    assert_eq!(stored.bambu_fun_bits.as_deref(), Some("8000004100000020"));
    assert_eq!(stored.bambu_fun_session_id.as_deref(), Some(session_two));

    let before_feature_only = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    let outcome = printers
        .update_device_features_if_current(
            tenant.id,
            agent.id,
            session_two,
            "FEATURE-SERIAL",
            Some(BambuDeviceFeatures::from_bits(FEATURE_ONLY_BITS)),
        )
        .await
        .unwrap();
    assert_eq!(outcome, DeviceFeatureUpdateOutcome::Updated);
    let after_feature_only = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    let mut expected = before_feature_only;
    expected.bambu_fun_bits = Some("8000004100000021".to_owned());
    expected.bambu_fun_session_id = Some(session_two.to_owned());
    assert_eq!(after_feature_only, expected);

    let outcome = printers
        .update_device_features_if_current(tenant.id, agent.id, session_two, "FEATURE-SERIAL", None)
        .await
        .unwrap();
    assert_eq!(outcome, DeviceFeatureUpdateOutcome::Updated);
    let after_invalidation = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    let mut invalidated = expected;
    invalidated.bambu_fun_session_id = None;
    assert_eq!(after_invalidation, invalidated);

    assert_eq!(
        printers
            .update_device_features_if_current(
                tenant.id,
                agent.id,
                session_two,
                "FEATURE-SERIAL",
                Some(BambuDeviceFeatures::from_bits(FULL_BITS)),
            )
            .await
            .unwrap(),
        DeviceFeatureUpdateOutcome::Updated
    );
    let unchanged = stored_printer(&database, tenant.id, "FEATURE-SERIAL").await;
    let other_session = "other-agent-session";
    claim_session(&agents, tenant.id, other_agent.id, other_session).await;

    let no_ops = [
        printers
            .update_secondary_device_features_if_current(
                tenant.id,
                agent.id,
                session_one,
                "FEATURE-SERIAL",
                Some(BambuDeviceFeatures::default()),
            )
            .await
            .unwrap(),
        printers
            .update_device_features_if_current(
                tenant.id,
                agent.id,
                session_one,
                "FEATURE-SERIAL",
                None,
            )
            .await
            .unwrap(),
        printers
            .update_device_features_if_current(
                other_tenant.id,
                agent.id,
                session_two,
                "FEATURE-SERIAL",
                Some(BambuDeviceFeatures::default()),
            )
            .await
            .unwrap(),
        printers
            .update_device_features_if_current(
                tenant.id,
                other_agent.id,
                other_session,
                "FEATURE-SERIAL",
                None,
            )
            .await
            .unwrap(),
        printers
            .update_device_features_if_current(
                tenant.id,
                agent.id,
                session_two,
                "UNKNOWN-SERIAL",
                Some(BambuDeviceFeatures::default()),
            )
            .await
            .unwrap(),
    ];
    assert!(
        no_ops
            .into_iter()
            .all(|outcome| outcome == DeviceFeatureUpdateOutcome::StaleOrMissing)
    );
    assert_eq!(
        stored_printer(&database, tenant.id, "FEATURE-SERIAL").await,
        unchanged
    );
}

mod support;
use support::*;
