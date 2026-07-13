use super::*;

#[tokio::test]
async fn same_owner_session_replacement_duplicate_retry_does_not_clear_modules() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let old_token = register_test_session(&state, tenant_id, agent_id).await;
    let serial = "SERIAL1";
    for event in [
        snapshot_event(tenant_id, agent_id, serial, "printer", "P2S", "idle"),
        firmware_event(
            tenant_id,
            agent_id,
            "old-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 1,
            }),
        ),
        firmware_event(
            tenant_id,
            agent_id,
            "old-modules",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.to_owned(),
                generation: 1,
                module_revision: 1,
                modules: vec![module("ota", "01.00")],
            }),
        ),
    ] {
        handle_event(&state, tenant_id, agent_id, old_token, event)
            .await
            .unwrap();
    }
    let printer_id = printer_id_for_serial(&state, tenant_id, serial).await;
    let replacement = register_test_session(&state, tenant_id, agent_id).await;

    for event in [
        firmware_event(
            tenant_id,
            agent_id,
            "replacement-initial-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 2,
            }),
        ),
        snapshot_event(tenant_id, agent_id, serial, "printer", "P2S", "ready"),
        firmware_event(
            tenant_id,
            agent_id,
            "replacement-retry-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 2,
            }),
        ),
        firmware_event(
            tenant_id,
            agent_id,
            "replacement-modules",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.to_owned(),
                generation: 2,
                module_revision: 1,
                modules: vec![module("ota", "02.00")],
            }),
        ),
        firmware_event(
            tenant_id,
            agent_id,
            "duplicate-retry-after-modules",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 2,
            }),
        ),
    ] {
        handle_event(&state, tenant_id, agent_id, replacement, event)
            .await
            .unwrap();
    }
    handle_event(
        &state,
        tenant_id,
        agent_id,
        old_token,
        firmware_event(
            tenant_id,
            agent_id,
            "late-old-session-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 3,
            }),
        ),
    )
    .await
    .unwrap();

    let current = current_printer(&state, tenant_id, &printer_id).await;
    assert_eq!(
        current.firmware.session_id,
        Some(replacement.persisted_id())
    );
    assert_eq!(current.firmware.generation, Some(2));
    assert_eq!(current.firmware.module_revision, 1);
    assert_eq!(
        current.firmware.modules.unwrap()[0]
            .software_version
            .as_deref(),
        Some("02.00")
    );
}
