use super::*;

#[tokio::test]
async fn printer_operation_required_features_reach_gateway_as_typed_axis_semantics() {
    let config = test_config();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(4);
    let home_id = uuid::Uuid::new_v4().to_string();
    let move_id = uuid::Uuid::new_v4().to_string();

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command_with_required_features(
            home_id,
            "SERIAL1",
            vec![DeviceFeature::BambuMqttHoming as i32],
            Some(printer_operation::Operation::Home(HomeOperation {
                axes: Vec::new(),
            })),
        ),
    )
    .await
    .unwrap();
    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command_with_required_features(
            move_id,
            "SERIAL1",
            vec![DeviceFeature::BambuMqttAxisControl as i32],
            Some(printer_operation::Operation::MoveAxes(MoveAxesOperation {
                movements: vec![AxisMovement {
                    axis: Axis::Y as i32,
                    delta_mm: -10.0,
                }],
                feedrate_mm_per_min: 0,
            })),
        ),
    )
    .await
    .unwrap();
    drop(sender);
    while receiver.recv().await.is_some() {}

    assert_eq!(
        gateway.operations().await,
        vec![
            (
                "SERIAL1".to_string(),
                MachinePrinterOperation::Home {
                    axes: Vec::new(),
                    required_feature: Some(BambuDeviceFeature::MqttHoming),
                },
            ),
            (
                "SERIAL1".to_string(),
                MachinePrinterOperation::MoveAxes {
                    x_mm: None,
                    y_mm: Some(-10.0),
                    z_mm: None,
                    feedrate_mm_per_min: None,
                    required_feature: Some(BambuDeviceFeature::MqttAxisControl),
                },
            ),
        ]
    );
}

#[tokio::test]
async fn printer_operation_invalid_hotend_temperature_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        hotend_operation_command(command_id.clone(), "SERIAL1", 301, false),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("temperature"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}

#[tokio::test]
async fn printer_operation_publish_failure_emits_ack_then_failure_with_redacted_context() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::publish_failure("ACCESS-CODE-UNIQUE");
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        resume_operation_command(command_id.clone(), "SERIAL1"),
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
            assert!(!result.success);
            assert!(
                result
                    .error
                    .contains("dispatch printer operation resume to SERIAL1")
            );
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert!(!result.error.contains("ACCESS-CODE-UNIQUE"));
            assert_eq!(result.result_json, "");
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![("SERIAL1".to_string(), MachinePrinterOperation::Resume)]
    );
}

#[tokio::test]
async fn printer_operation_does_not_reject_missing_local_model() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        set_print_speed_operation_command(command_id.clone(), "SERIAL1", 4),
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
                    action: "set_print_speed".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    speed_mode: Some(4),
                    ..empty_operation_result()
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::SetPrintSpeed(4)
        )]
    );
}

#[tokio::test]
async fn printer_operation_parses_typed_fan_speed_control() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        set_fan_speed_operation_command(command_id.clone(), "SERIAL1", 2, 50, true),
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
            assert_eq!(
                operation_result(&result.result_json),
                TestPrinterOperationResult {
                    kind: "printer_operation".to_owned(),
                    action: "set_fan_speed".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    fan_index: Some(2),
                    speed_percent: Some(50),
                    airduct: Some(true),
                    ..empty_operation_result()
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(
        gateway.operations().await,
        vec![(
            "SERIAL1".to_string(),
            MachinePrinterOperation::SetFanSpeed {
                fan_index: 2,
                speed_percent: 50,
                airduct: true,
            }
        )]
    );
}
