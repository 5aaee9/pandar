use super::*;

#[tokio::test]
async fn printer_operation_hotend_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        hotend_operation_command(command_id.clone(), "SERIAL1", 215, true),
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
                    action: "set_hotend_temperature".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    temperature_celsius: Some(215),
                    wait: Some(true),
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
            MachinePrinterOperation::SetHotendTemperature {
                temperature_celsius: 215,
                wait: true,
                extruder_id: None,
            }
        )]
    );
}

#[tokio::test]
async fn printer_operation_targeted_hotend_dispatches_extruder_id() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        hotend_operation_command_with_extruder(command_id.clone(), "SERIAL1", 220, false, Some(1)),
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
                    action: "set_hotend_temperature".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    temperature_celsius: Some(220),
                    extruder_id: Some(1),
                    wait: Some(false),
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
            MachinePrinterOperation::SetHotendTemperature {
                temperature_celsius: 220,
                wait: false,
                extruder_id: Some(1),
            }
        )]
    );
}

#[tokio::test]
async fn printer_operation_bed_temperature_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        bed_temperature_operation_command(command_id.clone(), "SERIAL1", 75, false),
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
                    action: "set_bed_temperature".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    temperature_celsius: Some(75),
                    wait: Some(false),
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
            MachinePrinterOperation::SetBedTemperature {
                temperature_celsius: 75,
                wait: false,
            }
        )]
    );
}

#[tokio::test]
async fn printer_operation_chamber_temperature_dispatches_typed_details() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        chamber_temperature_operation_command(command_id.clone(), "SERIAL1", 45, false),
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
                    action: "set_chamber_temperature".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    temperature_celsius: Some(45),
                    wait: Some(false),
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
            MachinePrinterOperation::SetChamberTemperature {
                temperature_celsius: 45,
                wait: false,
            }
        )]
    );
}
