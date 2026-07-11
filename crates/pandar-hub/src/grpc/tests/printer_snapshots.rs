use std::collections::BTreeMap;

use serde::Serialize;
use tonic::Code;

use super::*;
use crate::{
    printer_events::{PrinterEvent, PrinterEventMaterialJson},
    protocol::agent::v1::PrinterSnapshot,
    repositories::{MaterialPatchInput, test_helpers::insert_printer_fixture},
};

#[tokio::test]
async fn grpc_printer_snapshot_persists_printer_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot(" SN-001 ", " X1 Carbon ", " X1C ", " idle "),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(printers.len(), 1);
    assert_eq!(printers[0].agent_id, agent_id);
    assert_eq!(printers[0].serial_number, "SN-001");
    assert_eq!(printers[0].name, "X1 Carbon");
    assert_eq!(printers[0].model.as_deref(), Some("X1C"));
    assert_eq!(printers[0].status, "idle");
    assert!(printers[0].last_seen_at.ends_with('Z'));
}

#[tokio::test]
async fn grpc_printer_snapshot_rejects_empty_serial() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot(" ", "X1 Carbon", "X1C", "idle"),
        )))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn stale_replaced_stream_snapshot_does_not_mutate_printer_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let (_new_stream, _new_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    old_sender
        .send(Ok(snapshot_event(
            tenant_id,
            agent_id,
            snapshot("SN-STALE", "Stale Printer", "X1C", "idle"),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn replacement_session_blocks_old_snapshot_commit() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_before(old_token);

    let old_state = state.clone();
    let old_write = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            snapshot_event(
                tenant_id,
                agent_id,
                snapshot("SN-RACE", "Stale Printer", "X1C", "printing"),
            ),
        )
        .await
    });
    paused.wait_until_reached().await;

    let (_replacement_stream, _replacement_sender) =
        connect_live(&state, vec![hello_event(tenant_id, agent_id)])
            .await
            .unwrap();
    paused.resume();
    old_write.await.unwrap().unwrap();

    assert!(
        state
            .printers()
            .list_for_tenant(tenant_id)
            .await
            .unwrap()
            .is_empty(),
        "the stale snapshot must be rejected before its database commit"
    );
    let current = state.sessions().get(agent_id).await.unwrap();
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(current.token.persisted_id())
    );
    assert_eq!(persisted.status, AgentStatus::Online.as_str());
}

#[tokio::test]
async fn replacement_waits_for_snapshot_that_already_owns_transition_lease() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_after(old_token);

    let old_state = state.clone();
    let old_write = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            snapshot_event(
                tenant_id,
                agent_id,
                snapshot("SN-LINEARIZED", "Linearized Printer", "X1C", "printing"),
            ),
        )
        .await
    });
    paused.wait_until_reached().await;

    let replacement_state = state.clone();
    let replacement_token = SessionToken::new();
    let mut waiting = crate::sessions::transition_pause::observe_waiting(replacement_token);
    let replacement = tokio::spawn(async move {
        register_test_session_with_token(
            &replacement_state,
            tenant_id,
            agent_id,
            replacement_token,
        )
        .await;
        replacement_token
    });
    waiting.wait_until_reached().await;
    assert!(
        !replacement.is_finished(),
        "replacement must wait for the already-linearized snapshot"
    );

    paused.resume();
    old_write.await.unwrap().unwrap();
    let replacement = replacement.await.unwrap();

    let printers = state.printers().list_for_tenant(tenant_id).await.unwrap();
    assert_eq!(printers.len(), 1);
    assert_eq!(printers[0].serial_number, "SN-LINEARIZED");
    assert_eq!(printers[0].status, "printing");
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
}

#[tokio::test]
async fn printer_snapshot_event_includes_latest_materials() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    state
        .materials()
        .upsert_from_patch(MaterialPatchInput {
            tenant_id,
            agent_id,
            printer_id: printer_id.clone(),
            serial_number: format!("serial-{printer_id}"),
            printer_materials_json: valid_material_patch("2026-07-02T00:00:00Z"),
        })
        .await
        .unwrap();
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        snapshot(&format!("serial-{printer_id}"), "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
    assert_eq!(printer.id, printer_id);
    assert!(printer.state_revision.is_some());
    let print = printer.print.as_ref().expect("enriched print state");
    assert_eq!(print.task_generation, 0);
    assert_eq!(print.error_generation, 0);
    assert_eq!(print.job_state, None);
    assert!(print.hms.is_empty());
    let materials = printer.materials.unwrap();
    assert_eq!(
        materials.ams_units,
        PrinterEventMaterialJson::Array(vec![PrinterEventMaterialJson::Object(BTreeMap::from([
            (
                "unit_id".to_owned(),
                PrinterEventMaterialJson::String("0".to_owned())
            ),
            (
                "trays".to_owned(),
                PrinterEventMaterialJson::Array(vec![PrinterEventMaterialJson::Object(
                    BTreeMap::from([
                        (
                            "tray_id".to_owned(),
                            PrinterEventMaterialJson::String("0".to_owned())
                        ),
                        (
                            "type".to_owned(),
                            PrinterEventMaterialJson::String("PLA".to_owned())
                        ),
                    ])
                )])
            ),
        ]))])
    );
}

#[tokio::test]
async fn printer_snapshot_event_includes_temperatures() {
    let state = fixture_state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;
    let mut snapshot = snapshot("SN-TEMP", "Printer", "X2D", "IDLE");
    snapshot.nozzle_temperatures = vec![
        crate::protocol::agent::v1::NozzleTemperature {
            label: "L".to_owned(),
            current_celsius: "41".to_owned(),
            target_celsius: "220".to_owned(),
            diameter_mm: "0.4".to_owned(),
            nozzle_type: "Hardened steel".to_owned(),
        },
        crate::protocol::agent::v1::NozzleTemperature {
            label: "R".to_owned(),
            current_celsius: "42".to_owned(),
            target_celsius: "230".to_owned(),
            diameter_mm: "0.6".to_owned(),
            nozzle_type: "Stainless steel".to_owned(),
        },
    ];
    snapshot.bed_temperature_celsius = "60".to_owned();
    snapshot.bed_target_temperature_celsius = "65".to_owned();
    snapshot.chamber_temperature_celsius = "32".to_owned();
    snapshot.active_nozzle = "R".to_owned();
    snapshot.chamber_light_on = Some(true);

    handle_snapshot(&state, tenant_id, agent_id, token, snapshot)
        .await
        .unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
    assert_eq!(printer.nozzle_temperatures[0].label.as_deref(), Some("L"));
    assert_eq!(
        printer.nozzle_temperatures[0].target_celsius.as_deref(),
        Some("220")
    );
    assert_eq!(
        printer.nozzle_temperatures[0].diameter_mm.as_deref(),
        Some("0.4")
    );
    assert_eq!(
        printer.nozzle_temperatures[0].nozzle_type.as_deref(),
        Some("Hardened steel")
    );
    assert_eq!(
        printer.nozzle_temperatures[1].diameter_mm.as_deref(),
        Some("0.6")
    );
    assert_eq!(
        printer.nozzle_temperatures[1].nozzle_type.as_deref(),
        Some("Stainless steel")
    );
    assert_eq!(printer.bed_temperature_celsius.as_deref(), Some("60"));
    assert_eq!(
        printer.bed_target_temperature_celsius.as_deref(),
        Some("65")
    );
    assert_eq!(printer.chamber_temperature_celsius.as_deref(), Some("32"));
    assert_eq!(printer.active_nozzle.as_deref(), Some("R"));
    assert_eq!(printer.chamber_light_on, Some(true));
}

pub(super) fn snapshot(serial: &str, name: &str, model: &str, state: &str) -> PrinterSnapshot {
    PrinterSnapshot {
        serial: serial.to_string(),
        host: "192.0.2.10".to_string(),
        access_code: "12345678".to_string(),
        name: name.to_string(),
        model: model.to_string(),
        state: state.to_string(),
        nozzle_temperatures: Vec::new(),
        active_nozzle: String::new(),
        bed_temperature_celsius: String::new(),
        bed_target_temperature_celsius: String::new(),
        chamber_temperature_celsius: String::new(),
        chamber_light_on: None,
    }
}

pub(super) fn snapshot_event(
    tenant_id: TenantId,
    agent_id: AgentId,
    snapshot: PrinterSnapshot,
) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrinterSnapshot(snapshot)),
    }
}

pub(super) fn valid_material_patch(observed_at: &str) -> String {
    serde_json::to_string(&TestMaterialPatch {
        kind: "printer_material_patch",
        observed_at,
        ams_units: vec![TestAmsUnit {
            unit_id: "0",
            trays: vec![TestMaterialPatchTray {
                tray_id: "0",
                material_type: "PLA",
            }],
        }],
        external_spools: Vec::<TestExternalSpool>::new(),
    })
    .unwrap()
}

#[derive(Serialize)]
struct TestMaterialPatch<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    observed_at: &'a str,
    ams_units: Vec<TestAmsUnit>,
    external_spools: Vec<TestExternalSpool>,
}

#[derive(Serialize)]
struct TestAmsUnit {
    unit_id: &'static str,
    trays: Vec<TestMaterialPatchTray>,
}

#[derive(Serialize)]
struct TestMaterialPatchTray {
    tray_id: &'static str,
    #[serde(rename = "type")]
    material_type: &'static str,
}

#[derive(Serialize)]
struct TestExternalSpool {}
