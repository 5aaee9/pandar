use super::*;

#[tokio::test]
async fn printer_firmware_postgres_matches_sqlite_when_configured() {
    let Some(database) = super::super::postgres::postgres_database().await else {
        eprintln!(
            "PANDAR_TEST_POSTGRES_URL is unset; real PostgreSQL firmware verification skipped"
        );
        return;
    };
    let fixture = FirmwareFixture::new(database.clone(), "postgres-firmware").await;
    fixture.claim("postgres-session-one").await;
    assert_eq!(
        fixture
            .printers
            .establish_generation_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "postgres-session-one",
                &fixture.serial,
                10,
            )
            .await
            .unwrap(),
        PrinterFirmwareUpdateOutcome::Applied
    );
    assert_eq!(
        fixture
            .printers
            .replace_modules_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "postgres-session-one",
                &fixture.serial,
                10,
                2,
                vec![firmware_module("ota", "1"), firmware_module("ota", "1")],
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
                "postgres-session-one",
                &fixture.serial,
                10,
                3,
                Some(upgrade_state("RUNNING")),
                Some("cfg".to_owned()),
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
                "postgres-session-one",
                &fixture.serial,
                10,
                3,
                None,
                None,
            )
            .await
            .unwrap(),
        PrinterFirmwareUpdateOutcome::Stale
    );
    let hydrated = fixture.firmware().await;
    assert_eq!(hydrated.session_id.as_deref(), Some("postgres-session-one"));
    assert_eq!(hydrated.generation, Some(10));
    assert_eq!(hydrated.module_revision, 2);
    assert_eq!(hydrated.status_revision, 3);
    assert_eq!(hydrated.modules.unwrap().len(), 2);
    assert_eq!(hydrated.upgrade_state, Some(upgrade_state("RUNNING")));
    assert_eq!(hydrated.cfg.as_deref(), Some("cfg"));

    fixture.claim("postgres-session-two").await;
    assert_eq!(
        fixture
            .printers
            .establish_generation_if_current(
                fixture.tenant_id,
                fixture.agent_id,
                "postgres-session-two",
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
            session_id: Some("postgres-session-two".to_owned()),
            generation: Some(1),
            module_revision: 0,
            status_revision: 0,
            modules: None,
            upgrade_state: None,
            cfg: None,
        }
    );
}
