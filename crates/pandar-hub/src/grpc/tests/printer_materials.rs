use super::*;
use crate::{grpc::printer_materials::handle_materials_snapshot, printer_events::PrinterEvent};
use pandar_protocol::agent::v1::PrinterMaterialsSnapshot;

mod support;

use support::*;

#[tokio::test]
async fn malformed_material_snapshot_event_is_dropped_without_closing_stream() {
    let state = fixture_state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        material_event(
            tenant_id,
            agent_id,
            PrinterMaterialsSnapshot {
                serial: "serial".to_owned(),
                printer_id: printer_id.clone(),
                printer_materials_json: "not json".to_owned(),
            },
        ),
    )
    .await
    .unwrap();
    assert!(
        state
            .materials()
            .latest_for_printer(tenant_id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );

    handle_event(
        &state,
        tenant_id,
        agent_id,
        token,
        crate::grpc::tests::printer_snapshots::snapshot_event(
            tenant_id,
            agent_id,
            crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
        ),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn replacement_session_blocks_old_material_snapshot_commit() {
    let state = fixture_state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let old_token = current_token(&state, agent_id).await;
    let mut paused = crate::sessions::transition_pause::install_before(old_token);
    let old_printer_id = printer_id.clone();

    let old_state = state.clone();
    let old_material = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            material_event(
                tenant_id,
                agent_id,
                PrinterMaterialsSnapshot {
                    serial: "serial".to_owned(),
                    printer_id: old_printer_id,
                    printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
                },
            ),
        )
        .await
    });
    paused.wait_until_reached().await;

    let replacement = register_test_session(&state, tenant_id, agent_id).await;
    paused.resume();
    old_material.await.unwrap().unwrap();

    assert!(
        state
            .materials()
            .latest_for_printer(tenant_id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
}

#[tokio::test]
async fn printer_materials_snapshot_upserts_and_publishes_sanitized_materials() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: sensitive_material_patch_json(),
        },
    )
    .await
    .unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
    assert!(printer.state_revision.is_some());
    assert!(printer.print.is_some());
    assert_eq!(
        printer.materials.as_ref().unwrap().observed_at,
        "2026-07-02T00:00:00Z"
    );
    assert!(
        !serde_json::to_string(&printer)
            .unwrap()
            .contains("access_token")
    );
    assert!(!serde_json::to_string(&printer).unwrap().contains("secret"));
}

#[tokio::test]
async fn printer_materials_snapshot_without_printer_id_resolves_by_agent_and_serial() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: String::new(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
    )
    .await
    .unwrap();

    let snapshot = state
        .materials()
        .latest_for_printer(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.serial_number, "serial");
    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
    assert_eq!(printer.id, printer_id);
    assert!(printer.materials.is_some());
}

#[tokio::test]
async fn printer_materials_snapshot_with_mismatched_printer_id_and_serial_is_dropped() {
    let state = fixture_state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        PrinterMaterialsSnapshot {
            serial: "other-serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
    )
    .await
    .unwrap();

    assert!(
        state
            .materials()
            .latest_for_printer(tenant_id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn printer_materials_snapshot_with_printer_owned_by_other_agent_or_tenant_is_dropped() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, _printer_id) = fixture_printer(&state).await;
    let (other_tenant_id, other_agent_id, other_printer_id) =
        fixture_printer_for_other_tenant_and_agent(&state).await;
    let token = current_token(&state, agent_id).await;
    let other_token = current_token(&state, other_agent_id).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: other_printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
    )
    .await
    .unwrap();

    assert!(
        state
            .materials()
            .latest_for_printer(tenant_id, &other_printer_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .materials()
            .latest_for_printer(other_tenant_id, &other_printer_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(receiver.try_recv().is_err());

    handle_materials_snapshot(
        &state,
        other_tenant_id,
        other_agent_id,
        other_token,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: other_printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:01:00Z"),
        },
    )
    .await
    .unwrap();
    assert!(
        state
            .materials()
            .latest_for_printer(other_tenant_id, &other_printer_id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn printer_materials_snapshot_event_local_failures_are_dropped_without_stream_close() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    for event in [
        PrinterMaterialsSnapshot {
            serial: String::new(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: String::new(),
        },
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: "not-a-uuid".to_owned(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: uuid::Uuid::new_v4().to_string(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("not-rfc3339"),
        },
    ] {
        handle_materials_snapshot(&state, tenant_id, agent_id, token, event)
            .await
            .unwrap();
    }

    assert!(
        state
            .materials()
            .latest_for_printer(tenant_id, &printer_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(receiver.try_recv().is_err());

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
    )
    .await
    .unwrap();
    assert!(receiver.recv().await.is_ok());
}

#[tokio::test]
async fn older_and_unchanged_material_events_do_not_publish_noop_printer_events() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;
    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
    )
    .await
    .unwrap();
    receiver.recv().await.unwrap();

    for observed_at in ["2026-07-01T00:00:00Z", "2026-07-02T00:00:00Z"] {
        handle_materials_snapshot(
            &state,
            tenant_id,
            agent_id,
            token,
            PrinterMaterialsSnapshot {
                serial: "serial".to_owned(),
                printer_id: printer_id.clone(),
                printer_materials_json: valid_material_patch(observed_at),
            },
        )
        .await
        .unwrap();
    }

    assert!(receiver.try_recv().is_err());
}
