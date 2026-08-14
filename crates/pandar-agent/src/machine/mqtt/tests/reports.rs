use super::*;

#[test]
fn interpretation_extracts_progress_and_diagnostics() {
    let report = detailed_progress_report();

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.serial, "01S00EXAMPLE");
    assert_eq!(progress.job_id.as_deref(), Some("job-123"));
    assert_eq!(progress.artifact_id.as_deref(), Some("artifact-456"));
    assert_eq!(progress.subtask_id.as_deref(), Some("artifact-456"));
    assert_eq!(progress.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(progress.percent, Some(42));
    assert_eq!(progress.speed_level, Some(3));
    assert_eq!(progress.remaining_time_minutes, Some(87));
    assert_eq!(progress.current_layer, Some(12));
    assert_eq!(progress.total_layers, Some(120));
    assert_eq!(progress.gcode_file.as_deref(), Some("plate_1.gcode"));
    assert_eq!(progress.subtask_name.as_deref(), Some("drawer-organizer"));
    assert_eq!(progress.diagnostics.len(), 2);
    assert_eq!(progress.diagnostics[0].kind, "print_error");
    assert_eq!(progress.diagnostics[0].severity, "error");
    assert_eq!(progress.diagnostics[0].message, "nozzle temperature error");
    assert_eq!(progress.diagnostics[1].kind, "hms");
    assert_eq!(progress.diagnostics[1].severity, "warning");
    assert_eq!(
        progress.diagnostics[1].code.as_deref(),
        Some("0300_0A00_0001_0002")
    );
    assert_eq!(progress.diagnostics[1].message, "fan speed is low");
    assert_eq!(progress.observed_at, "2026-06-22T00:00:00Z");
}

#[test]
fn print_report_diagnostic_payload_includes_raw_print_report() {
    let report = serde_json::json!({
        "print": {
            "gcode_state": "FAILED",
            "mc_percent": 0,
            "print_error": "nozzle mismatch",
            "reason": "reject_nozzle_mismatch",
            "result": "fail"
        }
    });

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.diagnostics.len(), 1);
    let payload = serde_json::to_value(&progress.diagnostics[0].payload).unwrap();
    assert_eq!(payload["print_error"], "nozzle mismatch");
    assert_eq!(payload["raw_print"]["gcode_state"], "FAILED");
    assert_eq!(payload["raw_print"]["reason"], "reject_nozzle_mismatch");
    assert_eq!(payload["raw_print"]["result"], "fail");
}

#[test]
fn interpretation_drops_out_of_range_numeric_values() {
    let report = out_of_range_progress_report();

    let progress = print_report_from_json(&endpoint(), &report);

    assert_eq!(progress.percent, None);
    assert_eq!(progress.remaining_time_minutes, None);
    assert_eq!(progress.current_layer, None);
    assert_eq!(progress.total_layers, None);
}

#[test]
fn print_job_report_event_sets_numeric_presence_booleans() {
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let progress = PrintReportProgress {
        serial: "01S00EXAMPLE".to_owned(),
        job_id: Some("job-123".to_owned()),
        job_attr: None,
        print_error: None,
        printer_job_id: None,
        artifact_id: None,
        subtask_id: None,
        gcode_state: Some("RUNNING".to_owned()),
        percent: Some(0),
        speed_level: Some(2),
        remaining_time_minutes: None,
        current_layer: Some(7),
        total_layers: None,
        gcode_file: None,
        subtask_name: None,
        hms: None,
        diagnostics: Vec::new(),
        observed_at: "2026-06-22T00:00:00Z".to_owned(),
        printer_materials_json: String::new(),
    };

    let event = print_job_report_event(&config, progress);

    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    let Some(agent_event::Event::PrintJobReport(PrintJobReport {
        percent,
        has_percent,
        remaining_time_minutes,
        has_remaining_time_minutes,
        current_layer,
        has_current_layer,
        total_layers,
        has_total_layers,
        speed_level,
        has_speed_level,
        hms,
        has_hms,
        printer_materials_json,
        ..
    })) = event.event
    else {
        panic!("expected print job report event");
    };
    assert_eq!(percent, 0);
    assert!(has_percent);
    assert_eq!(remaining_time_minutes, 0);
    assert!(!has_remaining_time_minutes);
    assert_eq!(current_layer, 7);
    assert!(has_current_layer);
    assert_eq!(total_layers, 0);
    assert!(!has_total_layers);
    assert_eq!(speed_level, 2);
    assert!(has_speed_level);
    assert!(hms.is_empty());
    assert!(!has_hms);
    assert!(printer_materials_json.is_empty());
}

#[test]
fn interpretation_populates_printer_materials_json() {
    let report = external_vt_tray_report(254, "GFL05", "#abcdef");

    let progress = print_report_from_json(&endpoint(), &report);
    let materials = material_patch_json(&progress.printer_materials_json);

    assert_eq!(materials.external_spools[0].external_id, "254");
    assert_eq!(
        materials.external_spools[0].filament_id.as_deref(),
        Some("GFL05")
    );
    assert_eq!(
        materials.external_spools[0].color.as_deref(),
        Some("ABCDEF")
    );
    assert_eq!(
        materials.active_tray,
        Some(TestActiveTray::External {
            external_id: "254".to_owned(),
            tray_id: "0".to_owned(),
            global_tray_id: None,
        })
    );
}

#[tokio::test]
async fn forward_print_reports_uses_transport_without_live_socket() {
    let transport = FakeMqttTransport::with_reports([print_job_progress_report(
        "job-123",
        "artifact-456",
        "RUNNING",
        55,
    )]);
    let (sender, mut receiver) = mpsc::channel(4);
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let endpoint = endpoint();
    let forwarder = tokio::spawn({
        let config = config.clone();
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move {
            forward_print_reports(
                &config,
                &transport,
                &endpoint,
                Duration::from_millis(1),
                &sender,
                &crate::machine::DeviceFeatureCache::default(),
            )
            .await
        }
    });

    let event = receiver.recv().await.unwrap();
    drop(receiver);
    forwarder.await.unwrap().unwrap();

    let Some(agent_event::Event::PrintJobReport(report)) = event.event else {
        panic!("expected print job report event");
    };
    assert_eq!(report.serial, "01S00EXAMPLE");
    assert_eq!(report.job_id, "job-123");
    assert_eq!(report.artifact_id, "artifact-456");
    assert_eq!(report.subtask_id, "artifact-456");
    assert_eq!(report.percent, 55);
    assert!(report.has_percent);
    assert!(report.printer_materials_json.is_empty());
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
}

#[tokio::test]
async fn forward_print_reports_emits_printer_snapshot_with_temperatures() {
    let transport =
        FakeMqttTransport::with_reports([print_temperature_report("RUNNING", 41, 220, 60, 32)]);
    let (sender, mut receiver) = mpsc::channel(4);
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let endpoint = endpoint();
    let task = tokio::spawn({
        let config = config.clone();
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move {
            forward_print_reports(
                &config,
                &transport,
                &endpoint,
                Duration::from_millis(50),
                &sender,
                &crate::machine::DeviceFeatureCache::default(),
            )
            .await
            .unwrap();
        }
    });

    assert!(matches!(
        receiver.recv().await.unwrap().event,
        Some(agent_event::Event::PrintJobReport(_))
    ));
    let second = timeout(Duration::from_millis(50), receiver.recv())
        .await
        .expect("expected printer snapshot event")
        .unwrap();
    task.abort();

    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = second.event else {
        panic!("expected printer snapshot event");
    };
    assert_eq!(snapshot.serial, "01S00EXAMPLE");
    assert!(snapshot.model.is_empty());
    assert!(!snapshot.telemetry_authoritative);
    assert_eq!(snapshot.state, "RUNNING");
    assert_eq!(snapshot.nozzle_temperatures[0].current_celsius, "41");
    assert_eq!(snapshot.nozzle_temperatures[0].target_celsius, "220");
    assert_eq!(snapshot.bed_temperature_celsius, "60");
    assert_eq!(snapshot.chamber_temperature_celsius, "32");
}

#[tokio::test]
async fn forward_print_reports_emits_printer_snapshot_for_chamber_target_only() {
    let transport = FakeMqttTransport::with_reports([chamber_target_report(45)]);
    let (sender, mut receiver) = mpsc::channel(4);
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let endpoint = endpoint();
    let task = tokio::spawn({
        let config = config.clone();
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move {
            forward_print_reports(
                &config,
                &transport,
                &endpoint,
                Duration::from_millis(50),
                &sender,
                &crate::machine::DeviceFeatureCache::default(),
            )
            .await
            .unwrap();
        }
    });

    assert!(matches!(
        receiver.recv().await.unwrap().event,
        Some(agent_event::Event::PrintJobReport(_))
    ));
    let second = timeout(Duration::from_millis(50), receiver.recv())
        .await
        .expect("expected chamber target printer snapshot event")
        .unwrap();
    task.abort();

    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = second.event else {
        panic!("expected printer snapshot event");
    };
    assert_eq!(snapshot.chamber_temperature_celsius, "");
    assert_eq!(snapshot.chamber_target_temperature_celsius, "45");
}

#[tokio::test]
async fn forward_print_reports_emits_material_snapshot_for_unsolicited_ams_report() {
    let config = AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
        artifact_root: ".".into(),
    };
    let transport = FakeMqttTransport::with_reports([ams_print_report("IDLE", "PLA", None, None)]);
    let (sender, mut receiver) = mpsc::channel(2);

    let task = tokio::spawn(async move {
        forward_print_reports(
            &config,
            &transport,
            &endpoint(),
            Duration::from_millis(50),
            &sender,
            &crate::machine::DeviceFeatureCache::default(),
        )
        .await
        .unwrap();
    });

    let first = receiver.recv().await.unwrap();
    assert!(matches!(
        first.event,
        Some(agent_event::Event::PrintJobReport(_))
    ));
    let second = receiver.recv().await.unwrap();
    let Some(agent_event::Event::PrinterSnapshot(snapshot)) = second.event else {
        panic!("expected partial printer snapshot before materials");
    };
    assert_eq!(snapshot.state, "IDLE");
    assert!(!snapshot.telemetry_authoritative);
    let third = receiver.recv().await.unwrap();
    assert_material_snapshot(third, "01S00EXAMPLE", None);
    task.abort();
}

fn assert_material_snapshot(event: AgentEvent, serial: &str, printer_id: Option<&str>) {
    assert_eq!(event.agent_id, "agent-id");
    assert_eq!(event.tenant_id, "tenant-id");
    match event.event.unwrap() {
        agent_event::Event::PrinterMaterialsSnapshot(snapshot) => {
            assert_eq!(snapshot.serial, serial);
            assert_eq!(snapshot.printer_id, printer_id.unwrap_or_default());
            let patch = material_patch_json(&snapshot.printer_materials_json);
            assert_eq!(patch.document_type, "printer_material_patch");
            assert_eq!(
                patch.ams_units[0].trays[0].material_type.as_deref(),
                Some("PLA")
            );
        }
        other => panic!("expected printer materials snapshot, got {other:?}"),
    }
}
