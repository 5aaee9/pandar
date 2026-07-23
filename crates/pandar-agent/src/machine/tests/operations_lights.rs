use super::*;

#[tokio::test]
async fn configured_operate_printer_publishes_pause_to_request_topic() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::Pause)
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload("pause", "", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_select_extruder_publishes_reference_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::SelectExtruder(1))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_select_extruder_payload(1, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_toggle_light_sends_bambu_studio_light_nodes() {
    let mqtt = FakeMqttTransport::with_reports([lights_report(&[("chamber_light", "on")])]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::ToggleLight)
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let pushall_sequence_id = dynamic_section_sequence_id(&published[0].payload, "pushing");
    let light_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_eq!(
        published,
        vec![
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light", "off", &light_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light2", "off", &light2_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_toggle_light_matches_bambu_studio_light_nodes() {
    let mqtt = FakeMqttTransport::with_reports([lights_report(&[
        ("chamber_light", "off"),
        ("chamber_light2", "off"),
    ])]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::ToggleLight)
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let pushall_sequence_id = dynamic_section_sequence_id(&published[0].payload, "pushing");
    let light_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_eq!(
        published,
        vec![
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light", "on", &light_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light2", "on", &light2_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_set_chamber_light_uses_requested_state() {
    let mqtt = FakeMqttTransport::with_reports([lights_report(&[("chamber_light", "on")])]);
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::SetChamberLight(true))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let pushall_sequence_id = dynamic_section_sequence_id(&published[0].payload, "pushing");
    let light_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_eq!(
        published,
        vec![
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_pushall_payload(&pushall_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light", "on", &light_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
            PublishedMqttCommand {
                topic: "device/SERIAL1/request".to_string(),
                payload: expected_light_payload("chamber_light2", "on", &light2_sequence_id),
                qos: BAMBU_MQTT_QOS,
            },
        ]
    );
}

#[tokio::test]
async fn configured_operate_printer_light_returns_primary_success_when_light2_fails() {
    let mqtt = FakeMqttTransport::with_reports_and_operation_reports_failed_led_node(
        [lights_report(&[("chamber_light", "off")])],
        "chamber_light2",
    );
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let result = gateway
        .operate_printer("SERIAL1", PrinterOperation::SetChamberLight(true))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let primary_sequence_id = dynamic_section_sequence_id(&published[1].payload, "system");
    let light2_sequence_id = dynamic_section_sequence_id(&published[2].payload, "system");
    assert_ne!(primary_sequence_id, light2_sequence_id);
    assert_eq!(
        result.sequence_id.as_deref(),
        Some(primary_sequence_id.as_str())
    );
    assert_eq!(
        operation_report(result.mqtt_report.as_ref().unwrap())
            .system
            .unwrap()
            .result,
        "success"
    );
    assert_eq!(result.error, None);
}

#[tokio::test]
async fn configured_operate_printer_returns_matching_mqtt_sequence_result() {
    let mqtt = FakeMqttTransport::with_operation_reports();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint_without_model("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    let result = gateway
        .operate_printer("SERIAL1", PrinterOperation::Pause)
        .await
        .unwrap();

    let sequence_id = dynamic_sequence_id(&mqtt.published_commands().await[0].payload);
    assert_eq!(result.sequence_id.as_deref(), Some(sequence_id.as_str()));
    assert_eq!(
        operation_report(result.mqtt_report.as_ref().unwrap())
            .print
            .unwrap()
            .result,
        "success"
    );
}
