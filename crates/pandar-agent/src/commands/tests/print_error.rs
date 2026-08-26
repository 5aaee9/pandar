use super::*;
use crate::machine::mqtt::PrintErrorAction as MachinePrintErrorAction;
use pandar_protocol::agent::v1::{
    HandlePrintErrorOperation, PrintErrorAction as ProtoPrintErrorAction,
};

#[tokio::test]
async fn native_print_error_all_actions_convert_to_typed_machine_operations() {
    for (proto_action, machine_action) in [
        (
            ProtoPrintErrorAction::Resume,
            MachinePrintErrorAction::Resume,
        ),
        (
            ProtoPrintErrorAction::Ignore,
            MachinePrintErrorAction::Ignore,
        ),
        (ProtoPrintErrorAction::Stop, MachinePrintErrorAction::Stop),
    ] {
        let config = test_config();
        let command_id = uuid::Uuid::new_v4().to_string();
        let gateway = OperationGateway::default();
        let (sender, mut receiver) = mpsc::channel(2);

        handle_command_with_gateway(
            &config,
            &gateway,
            &sender,
            native_print_error_command(command_id.clone(), proto_action as i32, 83_918_929),
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
                        action: "handle_print_error".to_owned(),
                        serial_number: "SERIAL1".to_owned(),
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
                "SERIAL1".to_owned(),
                MachinePrinterOperation::HandlePrintError {
                    error_action: machine_action,
                    print_error: 83_918_929,
                    printer_job_id: "job-7".to_owned(),
                    sequence_id: 20_042,
                },
            )]
        );
    }
}

#[tokio::test]
async fn native_print_error_rejects_unspecified_and_unknown_actions_before_dispatch() {
    for action in [ProtoPrintErrorAction::Unspecified as i32, i32::MAX] {
        assert_native_print_error_rejected(action, 83_918_929, "error_action").await;
    }
}

#[tokio::test]
async fn native_print_error_rejects_out_of_domain_codes_before_dispatch() {
    for print_error in [0, i32::MAX as u32 + 1] {
        assert_native_print_error_rejected(
            ProtoPrintErrorAction::Resume as i32,
            print_error,
            "print_error",
        )
        .await;
    }
}

fn native_print_error_command(
    command_id: String,
    error_action: i32,
    print_error: u32,
) -> HubCommand {
    printer_operation_command(
        command_id,
        "SERIAL1",
        Some(printer_operation::Operation::HandlePrintError(
            HandlePrintErrorOperation {
                error_action,
                print_error,
                printer_job_id: "job-7".to_owned(),
                sequence_id: 20_042,
            },
        )),
    )
}

async fn assert_native_print_error_rejected(
    error_action: i32,
    print_error: u32,
    expected_error: &str,
) {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        native_print_error_command(command_id.clone(), error_action, print_error),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains(expected_error), "{}", ack.error);
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}
