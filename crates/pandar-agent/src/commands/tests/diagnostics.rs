use tokio::sync::mpsc;

use super::*;

#[tokio::test]
async fn discover_printers_emits_success_with_structured_result_json() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok([]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        discover_command(command_id.clone()),
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
                serde_json::from_str::<serde_json::Value>(&result.result_json).unwrap(),
                serde_json::json!({"type": "printer_discovery", "printers": []})
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[tokio::test]
async fn diagnose_printer_emits_success_with_structured_problem_result() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::ok([]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        diagnose_command(command_id.clone(), "SERIAL1"),
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
            assert_eq!(result.error, "");
            let value: serde_json::Value = serde_json::from_str(&result.result_json).unwrap();
            assert_eq!(value["type"], "printer_diagnostic");
            assert_eq!(value["serial_number"], "SERIAL1");
            assert_eq!(value["overall"], "problem");
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[test]
fn diagnose_command_payload_contains_only_serial_number() {
    let access_code = "ACCESS-CODE-UNIQUE";
    let command = diagnose_command("command-1".to_owned(), "SERIAL1");
    let payload = format!("{command:?}");

    assert!(payload.contains("SERIAL1"));
    assert!(!payload.contains(access_code));
}
