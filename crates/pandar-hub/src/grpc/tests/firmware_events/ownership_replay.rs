use std::time::Duration;

use super::*;
use crate::{
    protocol::agent::v1::PrinterSnapshot, repositories::FirmwareCommandOwner,
    sessions::FirmwareCommandIdentity,
};

mod same_owner;
mod stale_generation;

#[tokio::test]
async fn cross_agent_relink_retries_same_generation_after_snapshot_without_replay_tracker() {
    let state = fixture_state().await;
    let (tenant_id, agent_a) = tenant_agent(&state).await;
    let token_a = register_test_session(&state, tenant_id, agent_a).await;
    let serial = "SERIAL1";

    for event in [
        snapshot_event(tenant_id, agent_a, serial, "agent a printer", "P2S", "idle"),
        firmware_event(
            tenant_id,
            agent_a,
            "agent-a-generation-1",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 1,
            }),
        ),
        firmware_event(
            tenant_id,
            agent_a,
            "agent-a-modules-1",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.to_owned(),
                generation: 1,
                module_revision: 1,
                modules: vec![module("ota", "01.00")],
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
            "pending-before-relink".to_owned(),
            test_audit_actor(),
        )
        .await
        .unwrap();
    let (waiter, mut cancelled) = tokio::sync::oneshot::channel();
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
                generation: 1,
            },
            waiter,
        );
    }

    let agent_b = paired_agent(&state, tenant_id, "agent b").await;
    let token_b = register_test_session(&state, tenant_id, agent_b.id).await;
    for event in [
        snapshot_event(
            tenant_id,
            agent_b.id,
            serial,
            "agent b printer",
            "X1C",
            "printing",
        ),
        firmware_event(
            tenant_id,
            agent_b.id,
            "agent-b-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 9,
            }),
        ),
        firmware_event(
            tenant_id,
            agent_b.id,
            "agent-b-modules",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.to_owned(),
                generation: 9,
                module_revision: 1,
                modules: vec![module("ota", "09.00")],
            }),
        ),
    ] {
        handle_event(&state, tenant_id, agent_b.id, token_b, event)
            .await
            .unwrap();
    }

    handle_event(
        &state,
        tenant_id,
        agent_a,
        token_a,
        firmware_event(
            tenant_id,
            agent_a,
            "agent-a-initial-generation-2",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 2,
            }),
        ),
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut cancelled)
            .await
            .is_err(),
        "unowned invalidation must not cancel a pending generation"
    );
    let before_reclaim = current_printer(&state, tenant_id, &printer_id).await;
    assert_eq!(before_reclaim.printer.agent_id, agent_b.id);
    assert_eq!(before_reclaim.firmware.generation, Some(9));

    handle_event(
        &state,
        tenant_id,
        agent_a,
        token_a,
        snapshot_event(tenant_id, agent_a, serial, "agent a relink", "P2S", "ready"),
    )
    .await
    .unwrap();
    let after_reclaim = current_printer(&state, tenant_id, &printer_id).await;
    assert_eq!(after_reclaim.printer.agent_id, agent_a);
    assert_ne!(
        after_reclaim.firmware.session_id.as_deref(),
        Some(token_a.persisted_id().as_str())
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut cancelled)
            .await
            .is_err(),
        "ownership snapshot alone must not cancel the prior firmware generation"
    );

    handle_event(
        &state,
        tenant_id,
        agent_a,
        token_a,
        firmware_event(
            tenant_id,
            agent_a,
            "agent-a-retry-generation-2",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.to_owned(),
                generation: 2,
            }),
        ),
    )
    .await
    .unwrap();
    let cancelled = tokio::time::timeout(Duration::from_millis(200), cancelled)
        .await
        .expect("applied invalidation retry must cancel the prior local generation")
        .unwrap();
    assert!(cancelled.is_err());
    handle_event(
        &state,
        tenant_id,
        agent_a,
        token_a,
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
    )
    .await
    .unwrap();

    for (late_agent, late_token, generation, version) in [
        (agent_b.id, token_b, 10, "10.00"),
        (agent_a, token_a, 1, "01.99"),
    ] {
        handle_event(
            &state,
            tenant_id,
            late_agent,
            late_token,
            firmware_event(
                tenant_id,
                late_agent,
                "late-generation",
                agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                    serial: serial.to_owned(),
                    generation,
                }),
            ),
        )
        .await
        .unwrap();
        handle_event(
            &state,
            tenant_id,
            late_agent,
            late_token,
            firmware_event(
                tenant_id,
                late_agent,
                "late-modules",
                agent_event::Event::PrinterFirmwareModulesSnapshot(
                    PrinterFirmwareModulesSnapshot {
                        serial: serial.to_owned(),
                        generation,
                        module_revision: 99,
                        modules: vec![module("ota", version)],
                    },
                ),
            ),
        )
        .await
        .unwrap();
    }

    let current = current_printer(&state, tenant_id, &printer_id).await;
    assert_eq!(current.printer.agent_id, agent_a);
    assert_eq!(current.firmware.session_id, Some(token_a.persisted_id()));
    assert_eq!(current.firmware.generation, Some(2));
    assert_eq!(current.firmware.module_revision, 1);
    assert_eq!(
        current.firmware.modules.unwrap()[0]
            .software_version
            .as_deref(),
        Some("02.00")
    );
}

fn snapshot_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    serial: &str,
    name: &str,
    model: &str,
    state: &str,
) -> AgentEvent {
    firmware_event(
        tenant_id,
        agent_id,
        "printer-snapshot",
        agent_event::Event::PrinterSnapshot(PrinterSnapshot {
            serial: serial.to_owned(),
            host: "192.0.2.10".to_owned(),
            access_code: "test-access-code".to_owned(),
            name: name.to_owned(),
            state: state.to_owned(),
            model: model.to_owned(),
            nozzle_temperatures: Vec::new(),
            bed_temperature_celsius: String::new(),
            bed_target_temperature_celsius: String::new(),
            chamber_temperature_celsius: String::new(),
            chamber_target_temperature_celsius: String::new(),
            active_nozzle: String::new(),
            chamber_light_on: None,
            device_features: None,
            connection_authoritative: false,
            telemetry_authoritative: false,
        }),
    )
}

async fn printer_id_for_serial(state: &AppState, tenant_id: TenantId, serial: &str) -> String {
    state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .into_iter()
        .find(|printer| printer.serial_number == serial)
        .unwrap()
        .id
}

async fn current_printer(
    state: &AppState,
    tenant_id: TenantId,
    printer_id: &str,
) -> crate::repositories::PrinterWithLiveStatus {
    state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, printer_id)
        .await
        .unwrap()
        .unwrap()
}
