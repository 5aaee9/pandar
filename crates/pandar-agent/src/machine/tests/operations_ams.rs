use super::*;

#[tokio::test]
async fn configured_operate_printer_ams_load_publishes_change_filament_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::AmsLoadFilament {
                ams_id: 0,
                slot_id: 1,
                global_tray_id: Some(1),
                external_id: None,
                extruder_id: Some(0),
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
            payload: expected_ams_change_filament_payload(0, 1, 1, Some(0), -1, -1, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_bed_temperature_publishes_reference_gcode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetBedTemperature {
                temperature_celsius: 75,
                wait: false,
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
            payload: expected_print_command_payload("gcode_line", "M140 S75", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_chamber_temperature_publishes_reference_gcode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetChamberTemperature {
                temperature_celsius: 45,
                wait: false,
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
            payload: expected_print_command_payload("gcode_line", "M141 S45", &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_waiting_chamber_temperature_preserves_reference_gcode() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::SetChamberTemperature {
                temperature_celsius: 45,
                wait: true,
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
                "M106 P2 S255\nM191 S45\nM106 P2 S0",
                &sequence_id,
            ),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_ams_reread_rfid_publishes_get_rfid_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::AmsRereadRfid {
                ams_id: 0,
                slot_id: 1,
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
            payload: expected_ams_get_rfid_payload(0, 1, &sequence_id),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_ams_reread_rfid_increments_sequence_id() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    for _ in 0..2 {
        gateway
            .operate_printer(
                "SERIAL1",
                PrinterOperation::AmsRereadRfid {
                    ams_id: 0,
                    slot_id: 1,
                },
            )
            .await
            .unwrap();
    }

    let published = mqtt.published_commands().await;
    let first = dynamic_sequence_id(&published[0].payload);
    let second = dynamic_sequence_id(&published[1].payload);

    assert_ne!(first, "0");
    assert_ne!(second, "0");
    assert_ne!(first, second);
}

#[tokio::test]
async fn configured_operate_printer_ams_unload_publishes_change_filament_unload_command() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    gateway
        .operate_printer(
            "SERIAL1",
            PrinterOperation::AmsUnloadFilament {
                ams_id: 0,
                slot_id: 1,
                global_tray_id: Some(1),
                external_id: None,
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
            payload: expected_ams_change_filament_payload(
                0,
                255,
                255,
                None,
                210,
                210,
                &sequence_id
            ),
            qos: BAMBU_MQTT_QOS,
        }]
    );
}

#[tokio::test]
async fn configured_operate_printer_unknown_serial_rejects_before_publish() {
    let mqtt = FakeMqttTransport::default();
    let transfer = FakeMachineFileTransfer::default();
    let gateway = ConfiguredBambuMachineGateway::with_file_transfer(
        vec![(endpoint("SERIAL1"), mqtt.clone(), transfer)],
        Duration::from_secs(1),
    );

    let err = gateway
        .operate_printer("UNKNOWN", PrinterOperation::Pause)
        .await
        .unwrap_err();

    assert!(format!("{err:#}").contains("UNKNOWN"));
    assert!(mqtt.published_commands().await.is_empty());
}
