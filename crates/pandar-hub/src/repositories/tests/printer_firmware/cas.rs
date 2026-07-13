use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::*;
use crate::entities::printers;

#[tokio::test]
async fn printer_firmware_generation_cas_accepts_new_session_and_strictly_newer_generation() {
    let fixture = FirmwareFixture::new(sqlite_database().await, "firmware-generation").await;
    fixture.claim("session-one").await;

    for (generation, expected) in [
        (9, PrinterFirmwareUpdateOutcome::Applied),
        (9, PrinterFirmwareUpdateOutcome::Stale),
        (8, PrinterFirmwareUpdateOutcome::Stale),
        (10, PrinterFirmwareUpdateOutcome::Applied),
    ] {
        assert_eq!(
            fixture
                .printers
                .establish_generation_if_current(
                    fixture.tenant_id,
                    fixture.agent_id,
                    "session-one",
                    &fixture.serial,
                    generation,
                )
                .await
                .unwrap(),
            expected
        );
    }

    fixture.claim("session-two").await;
    assert_eq!(
        fixture
            .printers
            .establish_generation_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "session-two",
                &fixture.serial,
                1,
            )
            .await
            .unwrap(),
        PrinterFirmwareUpdateOutcome::Applied
    );
    assert_eq!(
        fixture.firmware().await,
        PrinterFirmwareState {
            session_id: Some("session-two".to_owned()),
            generation: Some(1),
            module_revision: 0,
            status_revision: 0,
            modules: None,
            upgrade_state: None,
            cfg: None,
        }
    );
}

#[tokio::test]
async fn printer_firmware_snapshots_require_exact_session_generation_and_respective_revision() {
    let fixture = FirmwareFixture::new(sqlite_database().await, "firmware-cas").await;
    fixture.claim("current-session").await;
    fixture
        .printers
        .establish_generation_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            "current-session",
            &fixture.serial,
            7,
        )
        .await
        .unwrap();

    let modules = vec![
        firmware_module("ota", "01.00"),
        firmware_module("ams", "02.00"),
        firmware_module("ota", "01.00"),
    ];
    assert_eq!(
        fixture
            .printers
            .replace_modules_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                7,
                2,
                modules.clone(),
            )
            .await
            .unwrap(),
        PrinterFirmwareUpdateOutcome::Applied
    );
    assert_eq!(
        fixture
            .printers
            .replace_status_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                7,
                5,
                Some(upgrade_state("RUNNING")),
                Some("cfg-a".to_owned()),
            )
            .await
            .unwrap(),
        PrinterFirmwareUpdateOutcome::Applied
    );
    let current = fixture.firmware().await;
    assert_eq!(current.modules, Some(modules));
    assert_eq!(current.module_revision, 2);
    assert_eq!(current.status_revision, 5);
    assert_eq!(current.upgrade_state, Some(upgrade_state("RUNNING")));
    assert_eq!(current.cfg.as_deref(), Some("cfg-a"));

    for outcome in [
        fixture
            .printers
            .replace_modules_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                6,
                3,
                vec![firmware_module("wrong-generation", "0")],
            )
            .await
            .unwrap(),
        fixture
            .printers
            .replace_modules_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                7,
                2,
                vec![firmware_module("same-revision", "0")],
            )
            .await
            .unwrap(),
        fixture
            .printers
            .replace_status_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                7,
                4,
                None,
                None,
            )
            .await
            .unwrap(),
    ] {
        assert_eq!(outcome, PrinterFirmwareUpdateOutcome::Stale);
    }
    assert_eq!(fixture.firmware().await, current);

    fixture.claim("replacement-session").await;
    for outcome in [
        fixture
            .printers
            .replace_modules_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                7,
                3,
                vec![firmware_module("old-session", "0")],
            )
            .await
            .unwrap(),
        fixture
            .printers
            .replace_status_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "current-session",
                &fixture.serial,
                7,
                6,
                None,
                None,
            )
            .await
            .unwrap(),
    ] {
        assert_eq!(outcome, PrinterFirmwareUpdateOutcome::Stale);
    }
    assert_eq!(fixture.firmware().await, current);
}

#[tokio::test]
async fn printer_firmware_invalidation_clears_only_firmware_columns() {
    let fixture = FirmwareFixture::new(sqlite_database().await, "firmware-invalidation").await;
    fixture.claim("session").await;
    fixture
        .printers
        .establish_generation_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            "session",
            &fixture.serial,
            3,
        )
        .await
        .unwrap();
    fixture
        .printers
        .replace_modules_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            "session",
            &fixture.serial,
            3,
            8,
            vec![firmware_module("ota", "1")],
        )
        .await
        .unwrap();
    fixture
        .printers
        .replace_status_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            "session",
            &fixture.serial,
            3,
            9,
            Some(upgrade_state("RUNNING")),
            Some("cfg".to_owned()),
        )
        .await
        .unwrap();
    let before = stored_printer(&fixture).await;

    fixture
        .printers
        .establish_generation_if_current(
            fixture.tenant_id,
            fixture.agent_id,
            "session",
            &fixture.serial,
            4,
        )
        .await
        .unwrap();
    let after = stored_printer(&fixture).await;
    let mut expected = before;
    expected.firmware_modules_json = None;
    expected.firmware_upgrade_state_json = None;
    expected.firmware_cfg = None;
    expected.firmware_session_id = Some("session".to_owned());
    expected.firmware_generation = Some(4);
    expected.firmware_module_revision = 0;
    expected.firmware_status_revision = 0;
    assert_eq!(after, expected);
}

async fn stored_printer(fixture: &FirmwareFixture) -> printers::Model {
    printers::Entity::find()
        .filter(printers::Column::TenantId.eq(fixture.tenant_id.to_string()))
        .filter(printers::Column::SerialNumber.eq(&fixture.serial))
        .one(&fixture.database.sea_orm_connection())
        .await
        .unwrap()
        .unwrap()
}
