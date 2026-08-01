use super::*;

#[tokio::test]
async fn refresh_subscribes_publishes_and_maps_report() {
    let mut endpoint = endpoint();
    endpoint.model = Some("Configured Model".to_string());
    let transport = FakeMqttTransport::with_reports([
        get_version_report("P2S"),
        print_gcode_state_report("RUNNING"),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint, Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(
        refreshed.snapshot,
        MachineSnapshot {
            serial: "01S00EXAMPLE".to_string(),
            host: Some("192.0.2.10".to_string()),
            access_code: Some("12345678".to_string()),
            name: "garage-a1".to_string(),
            model: Some("P2S".to_string()),
            state: Some("RUNNING".to_string()),
            nozzle_temperatures: Vec::new(),
            active_nozzle: None,
            bed_temperature_celsius: None,
            bed_target_temperature_celsius: None,
            chamber_temperature_celsius: None,
            chamber_target_temperature_celsius: None,
            chamber_light_on: None,
            device_features: None,
            device_features2: None,
            nozzle_system: None,
            telemetry_authoritative: true,
        }
    );
    assert_eq!(
        transport.subscriptions().await,
        ["device/01S00EXAMPLE/report".to_string()]
    );
    let published = transport.published_commands().await;
    let get_version_sequence_id = studio_sequence_id(&published[0].payload, "info");
    let pushall_sequence_id = studio_sequence_id(&published[1].payload, "pushing");
    assert_eq!(
        published,
        [
            request_command(expected_get_version_payload(&get_version_sequence_id)),
            request_command(expected_pushall_payload(&pushall_sequence_id)),
        ]
    );
}

#[tokio::test]
async fn refresh_printer_returns_material_patch_when_pushall_report_has_ams() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        ams_print_report("IDLE", "PLA", Some("FF0000"), Some("0")),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(refreshed.snapshot.serial, "01S00EXAMPLE");
    let materials = refreshed.materials.unwrap();
    let patch = material_patch_json(&materials.printer_materials_json);
    assert_eq!(patch.document_type, "printer_material_patch");
    assert_eq!(
        patch.ams_units[0].trays[0].material_type.as_deref(),
        Some("PLA")
    );
}

#[tokio::test]
async fn refresh_printer_keeps_first_snapshot_and_continues_until_ams_patch() {
    let transport = FakeMqttTransport::with_reports([
        get_version_report("A1 Mini"),
        print_gcode_state_report("IDLE"),
        ams_print_report("IDLE", "PLA", None, None),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(refreshed.snapshot.state.as_deref(), Some("IDLE"));
    assert!(refreshed.materials.is_some());
}

#[tokio::test]
async fn material_refresh_uses_total_deadline_for_infinite_non_ams_reports() {
    let transport = FakeMqttTransport::with_infinite_unrelated_reports();

    let err = refresh_printer_materials(&transport, &endpoint(), None, Duration::from_millis(10))
        .await
        .unwrap_err();

    let error = format!("{err:#}");
    assert!(error.contains("no AMS material report received before timeout"));
}

#[tokio::test]
async fn refresh_ignores_unrelated_reports_before_get_version() {
    let transport = FakeMqttTransport::with_reports([
        print_gcode_state_report("STALE"),
        info_command_report("other"),
        get_version_report("X1 Carbon"),
        print_state_report("READY"),
    ]);

    let refreshed = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap();

    assert_eq!(refreshed.snapshot.model.as_deref(), Some("X1 Carbon"));
    assert_eq!(refreshed.snapshot.state.as_deref(), Some("READY"));
    let published = transport.published_commands().await;
    let get_version_sequence_id = studio_sequence_id(&published[0].payload, "info");
    let pushall_sequence_id = studio_sequence_id(&published[1].payload, "pushing");
    assert_eq!(
        published,
        [
            request_command(expected_get_version_payload(&get_version_sequence_id)),
            request_command(expected_pushall_payload(&pushall_sequence_id)),
        ]
    );
}

#[tokio::test]
async fn refresh_timeout_error_includes_serial_context() {
    let transport = FakeMqttTransport::with_timeout();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("refresh printer 01S00EXAMPLE"));
    assert!(format!("{err:#}").contains("wait for MQTT get_version report"));
    assert!(transport.published_commands().await.len() == 1);
}

#[tokio::test]
async fn refresh_fails_total_get_version_deadline_when_unrelated_reports_continue() {
    let transport = FakeMqttTransport::with_infinite_unrelated_reports();

    let err = refresh_printer(&transport, &endpoint(), Duration::from_millis(10))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("timed out waiting for MQTT get_version report"));
    let published = transport.published_commands().await;
    let sequence_id = studio_sequence_id(&published[0].payload, "info");
    assert_eq!(
        published,
        [request_command(expected_get_version_payload(&sequence_id))]
    );
}

#[tokio::test]
async fn refresh_missing_model_fails_before_pushall() {
    let transport = FakeMqttTransport::with_reports([get_version_report_with_blank_model()]);

    let err = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("missing ota product_name"));
    let published = transport.published_commands().await;
    let sequence_id = studio_sequence_id(&published[0].payload, "info");
    assert_eq!(
        published,
        [request_command(expected_get_version_payload(&sequence_id))]
    );
}

#[tokio::test]
async fn refresh_get_version_publish_failure_fails_before_pushall() {
    let transport = FakeMqttTransport::with_publish_failure(BambuMqttCommand::GetVersion.payload());

    let err = refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("publish get_version"));
    assert!(format!("{err:#}").contains("fake publish failure"));
    assert!(transport.published_commands().await.is_empty());
}

#[test]
fn refresh_discovery_failure_log_includes_serial_and_error_chain() {
    let transport = FakeMqttTransport::with_publish_failure(BambuMqttCommand::GetVersion.payload());

    let (logs, ()) = crate::test_tracing::capture_logs(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                refresh_printer(&transport, &endpoint(), Duration::from_secs(1))
                    .await
                    .unwrap_err();
            });
    });

    let captured = logs.contents();
    assert!(captured.contains("printer model discovery failed"));
    assert!(captured.contains("01S00EXAMPLE"));
    assert!(captured.contains("publish get_version to request topic"));
    assert!(captured.contains("fake publish failure"));
}
