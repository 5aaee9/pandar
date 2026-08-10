use super::*;

#[tokio::test]
async fn configured_operate_printer_print_speed_mode_4_publishes_to_request_topic() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer("SERIAL1", PrinterOperation::SetPrintSpeed(4))
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload("print_speed", "4", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_legacy_fan_speed_uses_studio_pwm_gcode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetFanSpeed {
                fan_index: 1,
                speed_percent: 50,
                airduct: false,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload("gcode_line", "M106 P1 S128", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_airduct_fan_speed_uses_studio_set_fan() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetFanSpeed {
                fan_index: 2,
                speed_percent: 60,
                airduct: true,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: serde_json::json!({
                "print": {
                    "command": "set_fan",
                    "sequence_id": sequence_id,
                    "fan_index": 2,
                    "speed": 60
                }
            }),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_gcode_line_preserves_exact_param() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );
    let param = "M106 P1 S127 \r\n; keep  \n\n";

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::GcodeLine {
                param: param.to_owned(),
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload("gcode_line", param, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_home_preserves_axis_specific_request() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::Home {
                axes: vec![PrinterAxis::X, PrinterAxis::Z],
                required_feature: None,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload("gcode_line", "G28 X Z", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_move_axes_publishes_relative_gcode_line() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::MoveAxes {
                x_mm: Some(10.0),
                y_mm: None,
                z_mm: Some(-0.5),
                feedrate_mm_per_min: Some(3000.0),
                required_feature: None,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload(
                "gcode_line",
                "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 X10 Z-0.5 F3000\nM1002 pop_ref_mode\nM211 R",
                &sequence_id,
            ),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_hotend_publishes_wait_gcode_line() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetHotendTemperature {
                temperature_celsius: 215,
                wait: true,
                extruder_id: None,
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_print_command_payload("gcode_line", "M109 S215", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_targeted_hotend_publishes_reference_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
        TransferModeCache::default(),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetHotendTemperature {
                temperature_celsius: 220,
                wait: false,
                extruder_id: Some(1),
            },
        )
        .await
        .unwrap();

    let published = mqtt.published_commands().await;
    let sequence_id = dynamic_sequence_id(&published[0].payload);
    assert_eq!(
        published,
        vec![PublishedMqttCommand {
            topic: "device/SERIAL1/request".to_string(),
            payload: expected_targeted_hotend_payload(1, 220, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}
