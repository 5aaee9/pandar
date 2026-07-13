use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::*;
use crate::entities::printers;

const SQLITE_MIGRATION_PATH: &str = "migrations/sqlite/20260711010000_printer_firmware.sql";
const POSTGRES_MIGRATION_PATH: &str = "migrations/postgres/20260711010000_printer_firmware.sql";

#[test]
fn firmware_migration_sqlite_and_postgres_differ_only_by_integer_spelling() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let sqlite = std::fs::read_to_string(root.join(SQLITE_MIGRATION_PATH)).unwrap();
    let postgres = std::fs::read_to_string(root.join(POSTGRES_MIGRATION_PATH)).unwrap();

    assert_eq!(sqlite, postgres.replace("BIGINT", "INTEGER"));
}

#[tokio::test]
async fn printer_firmware_hydrates_absent_state_and_preserves_empty_modules() {
    let fixture = FirmwareFixture::new(sqlite_database().await, "firmware-hydration").await;

    assert_eq!(
        fixture.firmware().await,
        PrinterFirmwareState {
            session_id: None,
            generation: None,
            module_revision: 0,
            status_revision: 0,
            modules: None,
            upgrade_state: None,
            cfg: None,
        }
    );

    fixture.claim("hydration-session").await;
    assert_eq!(
        fixture
            .printers
            .establish_generation_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "hydration-session",
                &fixture.serial,
                4,
            )
            .await
            .unwrap(),
        PrinterFirmwareUpdateOutcome::Applied
    );
    fixture
        .printers
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            "hydration-session",
            &fixture.serial,
            4,
            1,
            Vec::new(),
        )
        .await
        .unwrap();
    assert_eq!(fixture.firmware().await.modules, Some(Vec::new()));
}

#[tokio::test]
async fn printer_firmware_malformed_typed_json_surfaces_complete_parse_cause() {
    let fixture = FirmwareFixture::new(sqlite_database().await, "firmware-malformed").await;
    let Database::Sqlite(pool) = &fixture.database else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE printers SET firmware_modules_json = ?2 WHERE id = ?1")
        .bind(&fixture.printer_id)
        .bind("{not-json")
        .execute(pool)
        .await
        .unwrap();

    let error = fixture
        .printers
        .get_with_live_status_for_tenant(fixture.tenant_id, &fixture.printer_id)
        .await
        .unwrap_err();
    let chain = format!("{error:#}");
    assert!(
        chain.contains("failed to rehydrate printer firmware"),
        "{chain}"
    );
    assert!(
        chain.contains("failed to read printer firmware modules"),
        "{chain}"
    );
    assert!(
        chain.contains("invalid type: map, expected a sequence"),
        "{chain}"
    );
}

#[tokio::test]
async fn firmware_migration_defaults_and_nonnegative_checks_hold_in_sqlite() {
    let fixture = FirmwareFixture::new(sqlite_database().await, "firmware-schema").await;
    let stored = printers::Entity::find_by_id(&fixture.printer_id)
        .one(&fixture.database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.firmware_modules_json, None);
    assert_eq!(stored.firmware_upgrade_state_json, None);
    assert_eq!(stored.firmware_cfg, None);
    assert_eq!(stored.firmware_session_id, None);
    assert_eq!(stored.firmware_generation, None);
    assert_eq!(stored.firmware_module_revision, 0);
    assert_eq!(stored.firmware_status_revision, 0);

    let Database::Sqlite(pool) = &fixture.database else {
        panic!("expected SQLite database");
    };
    let invalid_module_revision =
        sqlx::query("UPDATE printers SET firmware_module_revision = -1 WHERE id = ?1")
            .bind(&fixture.printer_id)
            .execute(pool)
            .await;
    assert!(invalid_module_revision.is_err());
    let invalid_status_revision =
        sqlx::query("UPDATE printers SET firmware_status_revision = -1 WHERE id = ?1")
            .bind(&fixture.printer_id)
            .execute(pool)
            .await;
    assert!(invalid_status_revision.is_err());

    let listed = printers::Entity::find()
        .filter(printers::Column::TenantId.eq(fixture.tenant_id.to_string()))
        .all(&fixture.database.sea_orm_connection())
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
}
