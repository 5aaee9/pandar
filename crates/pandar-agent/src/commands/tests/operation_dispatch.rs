use super::*;

#[tokio::test]
async fn printer_operation_select_extruder_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        select_extruder_operation_command(command_id.clone(), "SERIAL1", 1),
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
                    action: "select_extruder".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    extruder_id: Some(1),
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
            MachinePrinterOperation::SelectExtruder(1)
        )]
    );
}

#[tokio::test]
async fn printer_operation_move_axes_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        move_axes_operation_command(command_id.clone(), "SERIAL1"),
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
                    action: "move_axes".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    x_mm: Some(10.0),
                    z_mm: Some(-0.5),
                    feedrate_mm_per_min: Some(3000.0),
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
            MachinePrinterOperation::MoveAxes {
                x_mm: Some(10.0),
                y_mm: None,
                z_mm: Some(-0.5),
                feedrate_mm_per_min: Some(3000.0),
                required_feature: None,
            }
        )]
    );
}
