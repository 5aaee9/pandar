use super::*;
use crate::{
    grpc::printer_materials::handle_materials_snapshot, printer_events::PrinterEvent,
    protocol::agent::v1::PrinterMaterialsSnapshot,
};

#[tokio::test]
async fn malformed_material_snapshot_event_is_dropped_without_closing_stream() {
    let state = fixture_state().await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = SessionToken::new();

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
async fn printer_materials_snapshot_upserts_and_publishes_sanitized_materials() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: serde_json::json!({
                "type": "printer_material_patch",
                "observed_at": "2026-07-02T00:00:00Z",
                "ams_units": [{"unit_id": "0", "trays": [{"tray_id": "0", "type": "PLA", "access_token": "secret"}]}],
                "external_spools": []
            })
            .to_string(),
        },
    )
    .await
    .unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
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
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
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

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
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
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
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
        handle_materials_snapshot(&state, tenant_id, agent_id, event)
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

#[tokio::test(flavor = "current_thread")]
async fn printer_materials_snapshot_event_local_failures_are_logged() {
    let logs = super::log_capture::CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let missing_printer_id = uuid::Uuid::new_v4().to_string();

    let _guard = tracing::subscriber::set_default(subscriber);
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
            printer_id: missing_printer_id,
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
        PrinterMaterialsSnapshot {
            serial: "other-serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json:
                r#"{"type":"printer_material_patch","observed_at":"bad","password":"secret"}"#
                    .to_owned(),
        },
    ] {
        handle_materials_snapshot(&state, tenant_id, agent_id, event)
            .await
            .unwrap();
    }
    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-03T00:00:00Z"),
        },
    )
    .await
    .unwrap();
    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id: printer_id.clone(),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        },
    )
    .await
    .unwrap();
    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        PrinterMaterialsSnapshot {
            serial: "serial".to_owned(),
            printer_id,
            printer_materials_json: valid_material_patch("2026-07-03T00:00:00Z"),
        },
    )
    .await
    .unwrap();
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("ignored material snapshot event"));
    for reason in [
        "blank_serial",
        "blank_materials",
        "malformed_printer_id",
        "unknown_printer",
        "serial_mismatch",
        "invalid_patch",
        "older_patch",
        "unchanged_patch",
    ] {
        assert!(
            captured.contains(reason),
            "missing log reason {reason}: {captured}"
        );
    }
    assert!(captured.contains("invalid material patch JSON"));
    assert!(!captured.contains("secret"));
}

#[tokio::test]
async fn older_and_unchanged_material_events_do_not_publish_noop_printer_events() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;
    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
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

async fn fixture_printer(state: &AppState) -> (TenantId, AgentId, String) {
    let (tenant_id, agent_id) = tenant_agent(state).await;
    handle_snapshot(
        state,
        tenant_id,
        agent_id,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();
    let printer_id = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    (tenant_id, agent_id, printer_id)
}

async fn fixture_printer_for_other_tenant_and_agent(
    state: &AppState,
) -> (TenantId, AgentId, String) {
    let tenant = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let agent = paired_agent(state, tenant.id, "other-agent").await;
    handle_snapshot(
        state,
        tenant.id,
        agent.id,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();
    let printer_id = state
        .printers()
        .list_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    (tenant.id, agent.id, printer_id)
}

fn material_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterMaterialsSnapshot,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_owned(),
        event: Some(agent_event::Event::PrinterMaterialsSnapshot(snapshot)),
    }
}

fn valid_material_patch(observed_at: &str) -> String {
    crate::grpc::tests::printer_snapshots::valid_material_patch(observed_at)
}
