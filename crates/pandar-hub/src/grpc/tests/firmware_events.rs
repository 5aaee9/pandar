use pandar_core::{
    PrinterFirmwareModule as CoreFirmwareModule, PrinterUpgradeState as CoreUpgrade,
};
use tonic::Code;

use super::*;
use crate::{
    protocol::agent::v1::{
        PrinterFirmwareInvalidated, PrinterFirmwareModule, PrinterFirmwareModulesSnapshot,
        PrinterFirmwareStatusSnapshot, PrinterUpgradeState, agent_event,
    },
    repositories::test_helpers::insert_printer_fixture,
};

mod module_names;
mod ownership_replay;

#[tokio::test]
async fn firmware_event_uses_authenticated_stream_session_and_preserves_typed_payload() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let serial = format!("serial-{printer_id}");

    for event in [
        firmware_event(
            tenant_id,
            agent_id,
            "agent-supplied-session-id",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.clone(),
                generation: 3,
            }),
        ),
        firmware_event(
            tenant_id,
            agent_id,
            "another-agent-marker",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.clone(),
                generation: 3,
                module_revision: 1,
                modules: vec![module("ota", "01.00"), module("ota", "01.00")],
            }),
        ),
        firmware_event(
            tenant_id,
            agent_id,
            "untrusted-marker",
            agent_event::Event::PrinterFirmwareStatusSnapshot(PrinterFirmwareStatusSnapshot {
                serial: serial.clone(),
                generation: 3,
                status_revision: 2,
                upgrade_state: Some(PrinterUpgradeState {
                    status: Some("RUNNING".to_owned()),
                    progress: Some("25".to_owned()),
                    message: None,
                    module: Some("ota".to_owned()),
                    error_code: None,
                    new_version_state: None,
                    consistency_request: None,
                    force_upgrade: None,
                    display_state: None,
                    ota_new_version_number: None,
                    ams_new_version_number: None,
                    ahb_new_version_number: None,
                    new_versions: None,
                    ams_firmware: None,
                }),
                cfg: Some("cfg".to_owned()),
            }),
        ),
    ] {
        handle_event(&state, tenant_id, agent_id, token, event)
            .await
            .unwrap();
    }

    let firmware = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    assert_eq!(firmware.session_id, Some(token.persisted_id()));
    assert_eq!(firmware.generation, Some(3));
    assert_eq!(firmware.module_revision, 1);
    assert_eq!(firmware.status_revision, 2);
    assert_eq!(
        firmware.modules,
        Some(vec![
            core_module("ota", "01.00"),
            core_module("ota", "01.00")
        ])
    );
    assert_eq!(
        firmware.upgrade_state,
        Some(CoreUpgrade {
            status: Some("RUNNING".to_owned()),
            progress: Some("25".to_owned()),
            message: None,
            module: Some("ota".to_owned()),
            error_code: None,
            new_version_state: None,
            consistency_request: None,
            force_upgrade: None,
            display_state: None,
            ota_new_version_number: None,
            ams_new_version_number: None,
            ahb_new_version_number: None,
            new_versions: None,
            ams_firmware: None,
        })
    );
    assert_eq!(firmware.cfg.as_deref(), Some("cfg"));
}

#[tokio::test]
async fn firmware_event_rejects_values_that_do_not_fit_signed_storage() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;

    for event in [
        agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
            serial: "SERIAL".to_owned(),
            generation: i64::MAX as u64 + 1,
        }),
        agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
            serial: "SERIAL".to_owned(),
            generation: 1,
            module_revision: i64::MAX as u64 + 1,
            modules: Vec::new(),
        }),
        agent_event::Event::PrinterFirmwareStatusSnapshot(PrinterFirmwareStatusSnapshot {
            serial: "SERIAL".to_owned(),
            generation: i64::MAX as u64 + 1,
            status_revision: 1,
            upgrade_state: None,
            cfg: None,
        }),
    ] {
        let error = handle_event(
            &state,
            tenant_id,
            agent_id,
            token,
            firmware_event(tenant_id, agent_id, "event", event),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), Code::InvalidArgument);
        assert!(error.message().contains("i64::MAX"), "{error}");
    }
}

#[tokio::test]
async fn firmware_event_stale_session_and_generation_do_not_mutate_or_emit() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let old_token = register_test_session(&state, tenant_id, agent_id).await;
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let serial = format!("serial-{printer_id}");
    let mut events = state.printer_events().subscribe(tenant_id).await;
    handle_event(
        &state,
        tenant_id,
        agent_id,
        old_token,
        firmware_event(
            tenant_id,
            agent_id,
            "old-invalidation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.clone(),
                generation: 8,
            }),
        ),
    )
    .await
    .unwrap();
    handle_event(
        &state,
        tenant_id,
        agent_id,
        old_token,
        firmware_event(
            tenant_id,
            agent_id,
            "old-modules",
            agent_event::Event::PrinterFirmwareModulesSnapshot(PrinterFirmwareModulesSnapshot {
                serial: serial.clone(),
                generation: 8,
                module_revision: 1,
                modules: vec![module("ota", "8")],
            }),
        ),
    )
    .await
    .unwrap();
    let before = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;

    let replacement = register_test_session(&state, tenant_id, agent_id).await;
    handle_event(
        &state,
        tenant_id,
        agent_id,
        old_token,
        firmware_event(
            tenant_id,
            agent_id,
            "stale-session",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.clone(),
                generation: 9,
            }),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        state
            .printers()
            .get_with_live_status_for_tenant(tenant_id, &printer_id)
            .await
            .unwrap()
            .unwrap()
            .firmware,
        before
    );

    handle_event(
        &state,
        tenant_id,
        agent_id,
        replacement,
        firmware_event(
            tenant_id,
            agent_id,
            "replacement-invalidation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial: serial.clone(),
                generation: 1,
            }),
        ),
    )
    .await
    .unwrap();
    handle_event(
        &state,
        tenant_id,
        agent_id,
        replacement,
        firmware_event(
            tenant_id,
            agent_id,
            "same-generation",
            agent_event::Event::PrinterFirmwareInvalidated(PrinterFirmwareInvalidated {
                serial,
                generation: 1,
            }),
        ),
    )
    .await
    .unwrap();
    let current = state
        .printers()
        .get_with_live_status_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap()
        .firmware;
    assert_eq!(current.session_id, Some(replacement.persisted_id()));
    assert_eq!(current.generation, Some(1));
    assert_eq!(current.module_revision, 0);
    assert!(events.try_recv().is_err());
}

fn firmware_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    event_id: &str,
    event: agent_event::Event,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: event_id.to_owned(),
        event: Some(event),
    }
}

fn module(name: &str, version: &str) -> PrinterFirmwareModule {
    PrinterFirmwareModule {
        name: name.to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}

fn core_module(name: &str, version: &str) -> CoreFirmwareModule {
    CoreFirmwareModule {
        name: name.to_owned(),
        software_version: Some(version.to_owned()),
        software_new_version: None,
        new_version: None,
        visible: None,
        product_name: None,
        serial_number: None,
        hardware_version: None,
        firmware_flag: None,
    }
}
