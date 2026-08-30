use pandar_core::BambuDeviceFeatures;
use sea_orm::EntityTrait;

use super::*;
use crate::{
    db::Database,
    repositories::{MaterialPatchInput, MaterialPatchOutcome},
};

const PRIMARY_BITS: u64 = 0x101;
const SECONDARY_BITS: u64 = 0x202;
const REPLACEMENT_PRIMARY_BITS: u64 = 0x303;
const REPLACEMENT_SECONDARY_BITS: u64 = 0x404;

#[tokio::test]
async fn sqlite_printer_snapshot_applies_features_and_material_clear_atomically() {
    exercise_snapshot_success(sqlite_database().await).await;
}

#[tokio::test]
async fn postgres_printer_snapshot_applies_features_and_material_clear_atomically() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    exercise_snapshot_success(database.clone()).await;
}

#[tokio::test]
async fn sqlite_printer_snapshot_rolls_back_every_projection_on_material_failure() {
    exercise_snapshot_rollback(sqlite_database().await).await;
}

#[tokio::test]
async fn postgres_printer_snapshot_rolls_back_every_projection_on_material_failure() {
    let Some(database) = super::postgres::postgres_database().await else {
        eprintln!("skipping PostgreSQL test; PANDAR_TEST_POSTGRES_URL is not set");
        return;
    };
    exercise_snapshot_rollback(database.clone()).await;
}

async fn exercise_snapshot_success(database: Database) {
    let fixture = SnapshotFixture::new(database).await;
    let mut replacement =
        crate::repositories::tests::printer_device_features::support::rich_snapshot(
            &fixture.serial,
            "idle",
        );
    replacement.connection_authoritative = true;

    fixture
        .printers
        .apply_snapshot_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.session_id,
            replacement,
            Some(BambuDeviceFeatures::from_bits(REPLACEMENT_PRIMARY_BITS)),
            Some(BambuDeviceFeatures::from_bits(REPLACEMENT_SECONDARY_BITS)),
        )
        .await
        .unwrap();

    let stored = fixture.stored_printer().await;
    assert_eq!(stored.status, "idle");
    assert_eq!(
        stored.bambu_fun_bits.as_deref(),
        Some(
            BambuDeviceFeatures::from_bits(REPLACEMENT_PRIMARY_BITS)
                .to_hex()
                .as_str()
        )
    );
    assert_eq!(
        stored.bambu_fun2_bits.as_deref(),
        Some(
            BambuDeviceFeatures::from_bits(REPLACEMENT_SECONDARY_BITS)
                .to_hex()
                .as_str()
        )
    );
    assert!(
        fixture
            .materials
            .latest_for_printer(fixture.tenant_id, &fixture.printer_id)
            .await
            .unwrap()
            .is_none()
    );
}

async fn exercise_snapshot_rollback(database: Database) {
    let fixture = SnapshotFixture::new(database).await;
    install_material_delete_failure(&fixture.database).await;
    let mut replacement =
        crate::repositories::tests::printer_device_features::support::rich_snapshot(
            &fixture.serial,
            "idle",
        );
    replacement.connection_authoritative = true;

    let error = fixture
        .printers
        .apply_snapshot_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            &fixture.session_id,
            replacement,
            Some(BambuDeviceFeatures::from_bits(REPLACEMENT_PRIMARY_BITS)),
            Some(BambuDeviceFeatures::from_bits(REPLACEMENT_SECONDARY_BITS)),
        )
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains("forced aggregate snapshot rollback"));
    let stored = fixture.stored_printer().await;
    assert_eq!(stored.status, "printing");
    assert_eq!(
        stored.bambu_fun_bits.as_deref(),
        Some(
            BambuDeviceFeatures::from_bits(PRIMARY_BITS)
                .to_hex()
                .as_str()
        )
    );
    assert_eq!(
        stored.bambu_fun2_bits.as_deref(),
        Some(
            BambuDeviceFeatures::from_bits(SECONDARY_BITS)
                .to_hex()
                .as_str()
        )
    );
    assert!(
        fixture
            .materials
            .latest_for_printer(fixture.tenant_id, &fixture.printer_id)
            .await
            .unwrap()
            .is_some()
    );
}

struct SnapshotFixture {
    database: Database,
    printers: PrinterRepository,
    materials: MaterialRepository,
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    session_id: String,
    printer_id: String,
    serial: String,
}

impl SnapshotFixture {
    async fn new(database: Database) -> Self {
        let tenants = TenantRepository::new(database.clone());
        let agents = AgentRepository::new(database.clone());
        let printers = PrinterRepository::new(database.clone());
        let materials = MaterialRepository::new(database.clone());
        let suffix = uuid::Uuid::new_v4();
        let tenant = tenants
            .create(&format!("snapshot-atomic-{suffix}"), "Snapshot Atomic")
            .await
            .unwrap();
        let agent = agents.create(tenant.id, "agent").await.unwrap();
        let session_id = format!("snapshot-session-{suffix}");
        crate::repositories::tests::printer_device_features::support::claim_session(
            &agents,
            tenant.id,
            agent.id,
            &session_id,
        )
        .await;
        let serial = format!("ATOMIC-{suffix}");
        let initial = crate::repositories::tests::printer_device_features::support::rich_snapshot(
            &serial, "printing",
        );
        let printer = printers
            .apply_snapshot_if_current(
                tenant.id,
                agent.id,
                &session_id,
                initial,
                Some(BambuDeviceFeatures::from_bits(PRIMARY_BITS)),
                Some(BambuDeviceFeatures::from_bits(SECONDARY_BITS)),
            )
            .await
            .unwrap();
        let material = materials
            .upsert_from_patch_outcome(MaterialPatchInput {
                tenant_id: tenant.id,
                agent_id: agent.id,
                printer_id: printer.id.clone(),
                serial_number: serial.clone(),
                printer_materials_json: r#"{"type":"printer_material_patch","observed_at":"2026-08-30T00:00:00Z","ams_units":[],"external_spools":[],"replace_external_spools":true}"#.to_owned(),
            })
            .await
            .unwrap();
        assert!(matches!(material, MaterialPatchOutcome::Changed(_)));
        Self {
            database,
            printers,
            materials,
            tenant_id: tenant.id,
            agent_id: agent.id,
            session_id,
            printer_id: printer.id,
            serial,
        }
    }

    async fn stored_printer(&self) -> crate::entities::printers::Model {
        crate::entities::printers::Entity::find_by_id(&self.printer_id)
            .one(&self.database.sea_orm_connection())
            .await
            .unwrap()
            .unwrap()
    }
}

async fn install_material_delete_failure(database: &Database) {
    match database {
        Database::Sqlite(pool) => {
            sqlx::raw_sql(
                "CREATE TRIGGER fail_aggregate_snapshot_material_clear \
                 BEFORE DELETE ON printer_material_snapshots \
                 BEGIN SELECT RAISE(ABORT, 'forced aggregate snapshot rollback'); END;",
            )
            .execute(pool)
            .await
            .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::raw_sql(
                "CREATE FUNCTION fail_aggregate_snapshot_material_clear() RETURNS trigger \
                 LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced aggregate snapshot rollback'; END $$; \
                 CREATE TRIGGER fail_aggregate_snapshot_material_clear \
                 BEFORE DELETE ON printer_material_snapshots FOR EACH STATEMENT \
                 EXECUTE FUNCTION fail_aggregate_snapshot_material_clear();",
            )
            .execute(pool)
            .await
            .unwrap();
        }
    }
}
