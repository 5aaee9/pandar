use super::*;

#[tokio::test]
async fn stale_unowned_invalidation_does_not_cancel_newer_pending_generation() {
    let state = fixture_state().await;
    let (tenant_id, agent_a) = tenant_agent(&state).await;
    let token_a = register_test_session(&state, tenant_id, agent_a).await;
    let serial = "SERIAL1";
    for event in [
        snapshot_event(tenant_id, agent_a, serial, "agent a printer", "P2S", "idle"),
        firmware_event(
            tenant_id,
            agent_a,
            "agent-a-generation-2",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 2,
            }),
        ),
        firmware_event(
            tenant_id,
            agent_a,
            "agent-a-modules-2",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.to_owned(),
                generation: 2,
                module_revision: 1,
                modules: vec![module("ota", "02.00")],
            }),
        ),
    ] {
        handle_event(&state, tenant_id, agent_a, token_a, event)
            .await
            .unwrap();
    }
    let printer_id = printer_id_for_serial(&state, tenant_id, serial).await;
    let command = state
        .commands()
        .create_firmware_refresh_sent_with_audit(
            tenant_id,
            &printer_id,
            agent_a,
            FirmwareCommandOwner {
                session_id: token_a.persisted_id(),
                instance_id: state.instance_id(),
            },
            "pending-generation-2".to_owned(),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let (waiter, mut pending) = tokio::sync::oneshot::channel();
    {
        let _lease = state
            .sessions()
            .transition_lease_for_session(agent_a, token_a)
            .await;
        state.sessions().begin_firmware_refresh_under_transition(
            FirmwareCommandIdentity {
                command_id: command.id,
                tenant_id,
                agent_id: agent_a,
                session_token: token_a,
                printer_id: printer_id.clone(),
                serial: serial.to_owned(),
                generation: 2,
            },
            waiter,
        );
    }

    let agent_b = paired_agent(&state, tenant_id, "agent b").await;
    let token_b = register_test_session(&state, tenant_id, agent_b.id).await;
    handle_event(
        &state,
        tenant_id,
        agent_b.id,
        token_b,
        snapshot_event(
            tenant_id,
            agent_b.id,
            serial,
            "agent b printer",
            "X1C",
            "printing",
        ),
    )
    .await
    .unwrap();

    handle_event(
        &state,
        tenant_id,
        agent_a,
        token_a,
        firmware_event(
            tenant_id,
            agent_a,
            "late-agent-a-generation-1",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 1,
            }),
        ),
    )
    .await
    .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut pending)
            .await
            .is_err(),
        "an unowned stale generation must not cancel the newer pending generation"
    );
    let current = current_printer(&state, tenant_id, &printer_id).await;
    assert_eq!(current.printer.agent_id, agent_b.id);
}
