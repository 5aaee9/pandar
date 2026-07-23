use super::*;

#[tokio::test]
async fn printer_operation_valid_emits_ack_and_success_with_result_json() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        pause_operation_command(command_id.clone(), "SERIAL1"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
            assert_eq!(
                operation_result(&result.result_json),
                TestPrinterOperationResult {
                    kind: "printer_operation".to_owned(),
                    action: "pause".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    ..empty_operation_result()
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![("SERIAL1".to_string(), MachinePrinterOperation::Pause)]
    );
}

#[tokio::test]
async fn printer_operation_gcode_line_reaches_gateway_without_normalization() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);
    let param = "M106 P1 S127 \r\n; keep  \n\n";

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(
            command_id.clone(),
            "SERIAL1",
            Some(printer_operation::Operation::GcodeLine(
                GcodeLineOperation {
                    param: param.to_owned(),
                },
            )),
        ),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            assert_eq!(operation_result(&result.result_json).action, "gcode_line");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::GcodeLine {
                param: param.to_owned(),
            },
        )]
    );
}

#[tokio::test]
async fn printer_operation_ams_reread_rfid_emits_material_snapshot_after_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::with_materials(material_result("SERIAL1", None));
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(
            command_id.clone(),
            "SERIAL1",
            Some(printer_operation::Operation::AmsRereadRfid(
                AmsRereadRfidOperation {
                    ams_id: 1,
                    slot_id: 2,
                },
            )),
        ),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_material_snapshot(receiver.recv().await.unwrap(), "SERIAL1", None);
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
            assert_eq!(
                operation_result(&result.result_json),
                TestPrinterOperationResult {
                    kind: "printer_operation".to_owned(),
                    action: "ams_reread_rfid".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    ams_id: Some(1),
                    slot_id: Some(2),
                    ..empty_operation_result()
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::AmsRereadRfid {
                ams_id: 1,
                slot_id: 2
            }
        )]
    );
}

#[tokio::test]
async fn printer_operation_ams_load_emits_material_snapshot_after_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::with_materials(material_result("SERIAL1", None));
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(
            command_id.clone(),
            "SERIAL1",
            Some(printer_operation::Operation::AmsLoadFilament(
                AmsLoadFilamentOperation {
                    ams_id: 1,
                    slot_id: 2,
                    global_tray_id: 6,
                    external_id: String::new(),
                    extruder_id: Some(0),
                },
            )),
        ),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_material_snapshot(receiver.recv().await.unwrap(), "SERIAL1", None);
    assert!(
        matches!(receiver.recv().await.unwrap().event, Some(agent_event::Event::CommandResult(result)) if result.success)
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn printer_operation_ams_unload_emits_material_snapshot_after_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::with_materials(material_result("SERIAL1", None));
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(
            command_id.clone(),
            "SERIAL1",
            Some(printer_operation::Operation::AmsUnloadFilament(
                AmsUnloadFilamentOperation {
                    ams_id: 1,
                    slot_id: 2,
                    global_tray_id: 6,
                    external_id: String::new(),
                    extruder_id: Some(0),
                },
            )),
        ),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_material_snapshot(receiver.recv().await.unwrap(), "SERIAL1", None);
    assert!(
        matches!(receiver.recv().await.unwrap().event, Some(agent_event::Event::CommandResult(result)) if result.success)
    );
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn printer_operation_unknown_serial_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::unknown_serial();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        pause_operation_command(command_id.clone(), "UNKNOWN"),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("UNKNOWN"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_invalid_speed_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        set_print_speed_operation_command(command_id.clone(), "SERIAL1", 5),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("speed_mode"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_unspecified_axis_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        home_operation_command(
            command_id.clone(),
            "SERIAL1",
            vec![Axis::Unspecified as i32],
        ),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("axis"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}
