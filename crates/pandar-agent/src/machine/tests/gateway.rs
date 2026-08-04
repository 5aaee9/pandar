use super::*;

#[tokio::test]
async fn noop_refresh_printers_returns_no_snapshots() {
    let gateway = NoopMachineGateway;

    assert_eq!(gateway.refresh_printers().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn configured_refresh_printers_refreshes_endpoints_sequentially() {
    let first =
        FakeMqttTransport::with_reports([get_version_report("P2S"), print_state_report("READY")]);
    let second = FakeMqttTransport::with_reports([
        get_version_report("X1 Carbon"),
        root_state_report("IDLE"),
    ]);
    let first_endpoint = endpoint("SERIAL1");
    let second_endpoint = endpoint("SERIAL2");
    let gateway = ConfiguredBambuMachineGateway::new(
        vec![
            (first_endpoint.clone(), first.clone()),
            (second_endpoint.clone(), second.clone()),
        ],
        Duration::from_secs(1),
    );

    let snapshots = gateway
        .refresh_printers()
        .await
        .unwrap()
        .into_iter()
        .map(|result| result.snapshot)
        .collect::<Vec<_>>();

    assert_eq!(
        snapshots,
        vec![
            MachineSnapshot {
                serial: "SERIAL1".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("12345678".to_string()),
                name: "printer-SERIAL1".to_string(),
                model: Some("P2S".to_string()),
                state: Some("READY".to_string()),
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
            },
            MachineSnapshot {
                serial: "SERIAL2".to_string(),
                host: Some("192.0.2.10".to_string()),
                access_code: Some("12345678".to_string()),
                name: "printer-SERIAL2".to_string(),
                model: Some("X1 Carbon".to_string()),
                state: Some("IDLE".to_string()),
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
            },
        ]
    );
    assert_eq!(
        first.subscriptions().await,
        [format!("device/{}/report", first_endpoint.serial)]
    );
    assert_eq!(
        second.subscriptions().await,
        [format!("device/{}/report", second_endpoint.serial)]
    );
    let published = first.published_commands().await;
    let get_version_sequence_id = dynamic_section_sequence_id(&published[0].payload, "info");
    let pushall_sequence_id = dynamic_section_sequence_id(&published[1].payload, "pushing");
    assert_eq!(
        published,
        [
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_get_version_payload(&get_version_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_gateway_construction_uses_runtime_ftps_without_network_io() {
    let mqtt = FakeMqttTransport::default();
    let gateway = ConfiguredBambuMachineGateway::new(
        vec![(endpoint("SERIAL1"), mqtt)],
        Duration::from_secs(1),
    );

    assert_eq!(gateway.configured_printer_count(), 1);
}

#[tokio::test]
async fn configured_print_project_file_uploads_and_publishes_project_file() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let endpoint = endpoint("SERIAL1");
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint.clone(), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .print_project_file("SERIAL1", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap();

    assert_eq!(
        transfer.recorded_requests(),
        vec![(
            ProtectedData,
            FileTransferRequest::print_upload(
                "plate.gcode.3mf",
                3,
                PrintUploadPolicy {
                    try_emmc_print: false,
                },
            )
        )]
    );
    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    let submission_id = dynamic_project_file_submission_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_project_file_payload(&sequence_id, &submission_id),
            qos: crate::machine::mqtt::BAMBU_MQTT_QOS,
        }]
    );
}

#[test]
fn print_project_remote_name_matches_studio_suffix_policy() {
    assert_eq!(pick_remote_name("Cube"), "Cube.gcode.3mf");
    assert_eq!(pick_remote_name("plate.3mf"), "plate.gcode.3mf");
    assert_eq!(pick_remote_name("plate.gcode.3mf"), "plate.gcode.3mf");
    assert_eq!(pick_remote_name("../bad/name.3mf"), "name.gcode.3mf");
}

#[tokio::test]
async fn configured_print_project_file_does_not_publish_when_upload_fails() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::with_protected_failure();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let err = gateway
        .print_project_file("SERIAL1", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap_err();
    let message = format!("{err:#}");

    assert!(message.contains("upload print artifact to SERIAL1"));
    assert!(message.contains("192.0.2.10"));
    assert!(message.contains("fake protected data failure"));
    let redacted = gateway.redact_error(&message);
    assert!(!redacted.contains("192.0.2.10"));
    assert!(redacted.contains("[REDACTED_PRINTER_HOST]"));
    assert!(mqtt.published_commands().await.is_empty());
}

#[tokio::test]
async fn configured_print_project_file_unknown_serial_rejects_before_upload() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer.clone())],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let err = gateway
        .print_project_file("UNKNOWN", &print_project_file(), b"abc".to_vec())
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("UNKNOWN"));
    assert!(transfer.recorded_requests().is_empty());
    assert!(mqtt.published_commands().await.is_empty());
}
