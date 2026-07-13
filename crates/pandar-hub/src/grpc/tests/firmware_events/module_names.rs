use super::*;

#[tokio::test]
async fn firmware_modules_snapshot_rejects_empty_name_but_preserves_whitespace_name() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let serial = format!("serial-{printer_id}");
    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        firmware_event(
            tenant_id,
            agent_id,
            "module-name-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.clone(),
                generation: 3,
            }),
        ),
    )
    .await
    .unwrap();
    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        firmware_event(
            tenant_id,
            agent_id,
            "module-name-baseline",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.clone(),
                generation: 3,
                module_revision: 1,
                modules: vec![module("ota", "baseline")],
            }),
        ),
    )
    .await
    .unwrap();

    let error = handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        firmware_event(
            tenant_id,
            agent_id,
            "module-name-empty",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.clone(),
                generation: 3,
                module_revision: 2,
                modules: vec![module("", "invalid")],
            }),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    let unchanged = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    assert_eq!(unchanged.module_revision, 1);
    assert_eq!(unchanged.modules.unwrap()[0].name, "ota");

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        firmware_event(
            tenant_id,
            agent_id,
            "module-name-whitespace",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial,
                generation: 3,
                module_revision: 2,
                modules: vec![module("   ", "preserved")],
            }),
        ),
    )
    .await
    .unwrap();
    let stored = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    assert_eq!(stored.module_revision, 2);
    assert_eq!(stored.modules.unwrap()[0].name, "   ");
}
