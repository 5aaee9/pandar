use serde::Serialize;

use super::*;
use crate::AppState;
use pandar_core::{AgentId, TenantId};
use pandar_protocol::agent::v1::{AgentEvent, PrinterMaterialsSnapshot, agent_event};

pub(super) async fn fixture_printer(state: &AppState) -> (TenantId, AgentId, String) {
    let (tenant_id, agent_id) = tenant_agent(state).await;
    let token = register_test_session(state, tenant_id, agent_id).await;
    handle_snapshot(
        state,
        tenant_id,
        agent_id,
        token,
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

pub(super) async fn fixture_printer_for_other_tenant_and_agent(
    state: &AppState,
) -> (TenantId, AgentId, String) {
    let tenant = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let agent = paired_agent(state, tenant.id, "other-agent").await;
    let token = register_test_session(state, tenant.id, agent.id).await;
    handle_snapshot(
        state,
        tenant.id,
        agent.id,
        token,
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

pub(super) async fn current_token(state: &AppState, agent_id: AgentId) -> SessionToken {
    state.sessions().get(agent_id).await.unwrap().token
}

pub(super) fn material_event(
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

pub(super) fn valid_material_patch(observed_at: &str) -> String {
    crate::grpc::tests::printer_snapshots::valid_material_patch(observed_at)
}

pub(super) fn sensitive_material_patch_json() -> String {
    serde_json::to_string(&SensitiveMaterialPatch {
        kind: "printer_material_patch",
        observed_at: "2026-07-02T00:00:00Z",
        ams_units: vec![SensitiveAmsUnit {
            unit_id: "0",
            trays: vec![SensitiveMaterialTray {
                tray_id: "0",
                material_type: "PLA",
                access_token: "secret",
            }],
        }],
        external_spools: Vec::<SensitiveExternalSpool>::new(),
    })
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn printer_materials_snapshot_event_local_failures_are_logged() {
    let logs = super::super::log_capture::CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id, printer_id) = fixture_printer(&state).await;
    let token = current_token(&state, agent_id).await;
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
        handle_materials_snapshot(&state, tenant_id, agent_id, token, event)
            .await
            .unwrap();
    }
    handle_materials_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
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
        token,
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
        token,
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

#[derive(Serialize)]
struct SensitiveMaterialPatch {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'static str,
    ams_units: Vec<SensitiveAmsUnit>,
    external_spools: Vec<SensitiveExternalSpool>,
}

#[derive(Serialize)]
struct SensitiveAmsUnit {
    unit_id: &'static str,
    trays: Vec<SensitiveMaterialTray>,
}

#[derive(Serialize)]
struct SensitiveMaterialTray {
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
    access_token: &'static str,
}

#[derive(Serialize)]
struct SensitiveExternalSpool {}
