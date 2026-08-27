use pandar_core::{PrintTransferFailure, PrintTransferPhase};

use super::*;

const ACCESS_CODE: &str = "ACCESS-CODE-UNIQUE";

#[tokio::test]
async fn print_transfer_failure_emits_redacted_phase_and_complete_cause() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::with_transfer_failure(
        ["SERIAL1"],
        PrintTransferPhase::DataConnection,
        format!(
            "start protected upload with access_code={ACCESS_CODE}: 522 SSL connection failed: session reuse required"
        ),
        ACCESS_CODE,
    );
    let reader =
        FakeArtifactReader::with_artifacts([("tenant/artifact/plate.3mf", b"abc".to_vec())]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_reader(
        &config,
        &gateway,
        &reader,
        &sender,
        print_command(command_id.clone(), "SERIAL1", "tenant/artifact/plate.3mf"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    let event = receiver.recv().await.unwrap();
    let agent_event::Event::CommandResult(result) = event.event.unwrap() else {
        panic!("expected command result");
    };
    assert!(!result.success);
    assert!(result.error.contains("dispatch print job job-1"));
    assert!(result.error.contains("FTPS data connection phase"));
    assert!(result.error.contains("522 SSL connection failed"));
    assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
    assert!(!result.error.contains(ACCESS_CODE));
    assert_eq!(
        serde_json::from_str::<PrintTransferFailure>(&result.result_json).unwrap(),
        PrintTransferFailure {
            phase: PrintTransferPhase::DataConnection,
            cause: result.error,
        }
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.prints.lock().await.is_empty());
}
