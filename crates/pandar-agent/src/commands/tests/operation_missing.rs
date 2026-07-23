use super::*;

#[tokio::test]
async fn printer_operation_missing_operation_rejects_ack_without_dispatch() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = OperationGateway::default();
    let (sender, mut receiver) = mpsc::channel(1);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        printer_operation_command(command_id.clone(), "SERIAL1", None),
    )
    .await
    .unwrap();
    drop(sender);

    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains("missing printer operation"));
        }
        other => panic!("expected command ack, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
    assert!(gateway.operations().await.is_empty());
}
