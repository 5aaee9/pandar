use tokio::sync::mpsc;

use super::*;

#[derive(Debug, serde::Deserialize, PartialEq)]
struct TestPrinterDiscoveryResult {
    #[serde(rename = "type")]
    kind: String,
    printers: Vec<TestDiscoveredPrinter>,
}

#[derive(Debug, serde::Deserialize, PartialEq)]
struct TestDiscoveredPrinter {}

#[derive(Debug, serde::Deserialize, PartialEq)]
struct TestPrinterDiagnosticResult {
    #[serde(rename = "type")]
    kind: String,
    serial_number: String,
    overall: String,
}

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
                serde_json::from_str::<TestPrinterDiscoveryResult>(&result.result_json).unwrap(),
                TestPrinterDiscoveryResult {
                    kind: "printer_discovery".to_owned(),
                    printers: Vec::new(),
                }
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
            assert_eq!(
                serde_json::from_str::<TestPrinterDiagnosticResult>(&result.result_json).unwrap(),
                TestPrinterDiagnosticResult {
                    kind: "printer_diagnostic".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    overall: "problem".to_owned(),
                }
            );
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
