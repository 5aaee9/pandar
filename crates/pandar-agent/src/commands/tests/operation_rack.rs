use super::*;

#[tokio::test]
async fn printer_operation_nozzle_holder_ctrl_dispatches_with_action() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(
            command_id.clone(),
            "SERIAL1",
            Some(printer_operation::Operation::NozzleHolderCtrl(
                NozzleHolderCtrlOperation { action: 2 },
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
            assert_eq!(
                operation_result(&result.result_json),
                TestPrinterOperationResult {
                    kind: "printer_operation".to_owned(),
                    action: "nozzle_holder_ctrl".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    holder_action: Some(2),
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
            MachinePrinterOperation::NozzleHolderCtrl { action: 2 }
        )]
    );
}

#[tokio::test]
async fn printer_operation_rack_nozzle_ops_dispatch_with_id() {
    for (operation, expected_action, expected_operation) in [
        (
            printer_operation::Operation::NozzleInfoConfirm(NozzleInfoConfirmOperation {
                id: 0xff,
            }),
            "nozzle_info_confirm",
            MachinePrinterOperation::NozzleInfoConfirm { id: 0xff },
        ),
        (
            printer_operation::Operation::HolderNozzleRefresh(HolderNozzleRefreshOperation {
                id: 17,
            }),
            "holder_nozzle_refresh",
            MachinePrinterOperation::HolderNozzleRefresh { id: 17 },
        ),
    ] {
        let config = test_config();
        let command_id = uuid::Uuid::new_v4().to_string();
        let gateway = OperationGateway::default();
        let (sender, mut receiver) = mpsc::channel(2);

        handle_command_with_gateway(
            &config,
            &gateway,
            &sender,
            printer_operation_command(command_id.clone(), "SERIAL1", Some(operation)),
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
                let result = operation_result(&result.result_json);
                assert_eq!(result.action, expected_action);
                assert_eq!(result.nozzle_id, expected_nozzle_id(&expected_operation));
            }
            other => panic!("expected command result, got {other:?}"),
        }
        assert_eq!(
            gateway.operations().await,
            vec![("SERIAL1".to_string(), expected_operation)]
        );
    }
}

#[tokio::test]
async fn printer_operation_invalid_rack_values_reject_ack_without_dispatch() {
    for (operation, expected_error) in [
        (
            printer_operation::Operation::NozzleHolderCtrl(NozzleHolderCtrlOperation { action: 3 }),
            "nozzle_holder_ctrl",
        ),
        (
            printer_operation::Operation::NozzleInfoConfirm(NozzleInfoConfirmOperation { id: 15 }),
            "nozzle_info_confirm",
        ),
        (
            printer_operation::Operation::HolderNozzleRefresh(HolderNozzleRefreshOperation {
                id: 22,
            }),
            "holder_nozzle_refresh",
        ),
    ] {
        let config = test_config();
        let command_id = uuid::Uuid::new_v4().to_string();
        let gateway = OperationGateway::default();
        let (sender, mut receiver) = mpsc::channel(1);

        handle_command_with_gateway(
            &config,
            &gateway,
            &sender,
            printer_operation_command(command_id.clone(), "SERIAL1", Some(operation)),
        )
        .await
        .unwrap();
        drop(sender);

        match receiver.recv().await.unwrap().event.unwrap() {
            agent_event::Event::CommandAck(ack) => {
                assert!(!ack.accepted);
                assert!(ack.error.contains(expected_error));
            }
            other => panic!("expected command ack, got {other:?}"),
        }
        assert!(receiver.recv().await.is_none());
        assert!(gateway.operations().await.is_empty());
    }
}

fn expected_nozzle_id(operation: &MachinePrinterOperation) -> Option<u32> {
    match operation {
        MachinePrinterOperation::NozzleInfoConfirm { id }
        | MachinePrinterOperation::HolderNozzleRefresh { id } => Some(*id),
        _ => None,
    }
}
