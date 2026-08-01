use super::*;

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
    assert_eq!(materials.filament_switch_installed, Some(true));
    assert_eq!(materials.cfg.as_deref(), Some("8000000000000001"));
    assert_eq!(materials.aux.as_deref(), Some("A4003001"));
    assert_eq!(materials.stat.as_deref(), Some("1000000001"));
    assert_eq!(
        materials.ams_units,
        PrinterEventMaterialJson::Array(vec![PrinterEventMaterialJson::Object(BTreeMap::from([
            (
                "info".to_owned(),
                PrinterEventMaterialJson::String("00000E00".to_owned())
            ),
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
            snow: None,
            hnow: None,
        },
        crate::protocol::agent::v1::NozzleTemperature {
            label: "R".to_owned(),
            current_celsius: "42".to_owned(),
            target_celsius: "230".to_owned(),
            diameter_mm: "0.6".to_owned(),
            nozzle_type: "Stainless steel".to_owned(),
            snow: None,
            hnow: None,
        },
    ];
    snapshot.bed_temperature_celsius = "60".to_owned();
    snapshot.bed_target_temperature_celsius = "65".to_owned();
    snapshot.chamber_temperature_celsius = "32".to_owned();
    snapshot.chamber_target_temperature_celsius = "45".to_owned();
    snapshot.active_nozzle = "R".to_owned();
    snapshot.chamber_light_on = Some(true);

    handle_snapshot(&state, tenant_id, agent_id, token, snapshot)
        .await
        .unwrap();

    let stored = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        stored.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );

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
    assert_eq!(
        printer.chamber_target_temperature_celsius.as_deref(),
        Some("45")
    );
    assert_eq!(printer.active_nozzle.as_deref(), Some("R"));
    assert_eq!(printer.chamber_light_on, Some(true));
}
