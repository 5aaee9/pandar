use std::path::PathBuf;

use serde::Deserialize;
use tokio::sync::mpsc;

use super::{assert_failure_contains, test_config};
use crate::{
    commands::{
        FilesystemArtifactReader, ack_event, handle_command_with_reader,
        handle_non_firmware_command_with_gateway as handle_command_with_gateway,
    },
    protocol::agent::v1::{AgentEvent, HubCommand, PrintProjectFile, agent_event, hub_command},
};

mod support;

use support::*;

#[derive(Debug, Deserialize, PartialEq)]
struct TestPrintProjectFileResult {
    #[serde(rename = "type")]
    kind: String,
    serial_number: String,
    job_id: String,
    artifact_id: String,
    uploaded_path: String,
    uploaded_url: String,
    md5: String,
    mqtt: TestPrintProjectMqttResult,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestPrintProjectMqttResult {
    topic: String,
    qos: u8,
    payload: TestPrintProjectMqttPayload,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestPrintProjectMqttPayload {
    print: TestPrintProjectPrintPayload,
}

#[derive(Debug, Deserialize, PartialEq)]
struct TestPrintProjectPrintPayload {
    command: String,
}

#[tokio::test]
async fn print_project_file_reads_artifact_reader_and_emits_ack_success() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
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
    assert_print_success(receiver.recv().await.unwrap(), &command_id);
    assert!(receiver.recv().await.is_none());
    assert_eq!(
        gateway.prints.lock().await.as_slice(),
        &[RecordedPrint {
            serial_number: "SERIAL1".to_string(),
            job_id: "job-1".to_string(),
            artifact: b"abc".to_vec(),
        }]
    );
    assert_eq!(
        reader.reads.lock().await.as_slice(),
        &["tenant/artifact/plate.3mf".to_string()]
    );
}

#[tokio::test]
async fn print_project_file_rejects_unsafe_artifact_path_before_gateway() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let reader = FakeArtifactReader::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_reader(
        &config,
        &gateway,
        &reader,
        &sender,
        print_command(command_id.clone(), "SERIAL1", "../plate.3mf"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(receiver.recv().await.unwrap(), &command_id, "storage path");
    assert!(gateway.prints.lock().await.is_empty());
    assert_eq!(reader.reads.lock().await.as_slice(), &["../plate.3mf"]);
}

#[tokio::test]
async fn print_project_file_missing_artifact_fails_with_storage_path_context() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let reader = FakeArtifactReader::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_reader(
        &config,
        &gateway,
        &reader,
        &sender,
        print_command(command_id.clone(), "SERIAL1", "tenant/artifact/missing.3mf"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(
        receiver.recv().await.unwrap(),
        &command_id,
        "tenant/artifact/missing.3mf",
    );
    assert!(gateway.prints.lock().await.is_empty());
    assert_eq!(
        reader.reads.lock().await.as_slice(),
        &["tenant/artifact/missing.3mf".to_string()]
    );
}

#[tokio::test]
async fn hub_artifact_download_failure_does_not_report_artifact_paths() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 2048];
        let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut socket,
            b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\r\n",
        )
        .await
        .unwrap();
    });
    let config = crate::AgentConfig {
        hub_api_url: Some(base_url),
        ..test_config()
    };
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        print_command(command_id.clone(), "SERIAL1", "tenant/artifact/secret.3mf"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_excludes(
        receiver.recv().await.unwrap(),
        &command_id,
        &[
            "tenant/artifact/secret.3mf",
            "/api/v1/agents/agent-1/artifacts/artifact-1",
        ],
    );
    assert!(gateway.prints.lock().await.is_empty());
    server.abort();
}

#[tokio::test]
async fn print_project_file_unknown_serial_rejects_before_artifact_read() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let reader = FakeArtifactReader::default();
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_reader(
        &config,
        &gateway,
        &reader,
        &sender,
        print_command(command_id.clone(), "UNKNOWN", "tenant/artifact/missing.3mf"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_rejected_ack_contains(receiver.recv().await.unwrap(), &command_id, "UNKNOWN");
    assert!(receiver.recv().await.is_none());
    assert!(gateway.prints.lock().await.is_empty());
    assert!(reader.reads.lock().await.is_empty());
}

#[tokio::test]
async fn filesystem_artifact_reader_reads_relative_path_under_configured_root() {
    let temp_dir = temp_artifact_root();
    std::fs::create_dir_all(temp_dir.join("tenant/artifact")).unwrap();
    std::fs::write(temp_dir.join("tenant/artifact/plate.3mf"), b"abc").unwrap();
    let config = crate::AgentConfig {
        artifact_root: temp_dir,
        ..test_config()
    };
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakePrintGateway::ok(["SERIAL1"]);
    let reader = FilesystemArtifactReader::new(config.artifact_root.clone());
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
    assert_print_success(receiver.recv().await.unwrap(), &command_id);
    assert_eq!(
        gateway.prints.lock().await.as_slice(),
        &[RecordedPrint {
            serial_number: "SERIAL1".to_string(),
            job_id: "job-1".to_string(),
            artifact: b"abc".to_vec(),
        }]
    );
}

fn print_command(command_id: String, serial_number: &str, storage_path: &str) -> HubCommand {
    HubCommand {
        command_id,
        command: Some(hub_command::Command::PrintProjectFile(PrintProjectFile {
            job_id: "job-1".to_string(),
            artifact_id: "artifact-1".to_string(),
            printer_id: "printer-1".to_string(),
            serial_number: serial_number.to_string(),
            filename: "plate.3mf".to_string(),
            storage_path: storage_path.to_string(),
            artifact_download_path: "/api/v1/agents/agent-1/artifacts/artifact-1".to_string(),
            size_bytes: 3,
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: true,
            ams_mapping_json: String::new(),
            ams_mapping2_json: String::new(),
            ams_mapping_info_json: String::new(),
        })),
    }
}

fn assert_rejected_ack_contains(event: AgentEvent, command_id: &str, needle: &str) {
    match event.event.unwrap() {
        agent_event::Event::CommandAck(ack) => {
            assert_eq!(ack.command_id, command_id);
            assert!(!ack.accepted);
            assert!(ack.error.contains(needle), "{}", ack.error);
        }
        other => panic!("expected command ack, got {other:?}"),
    }
}

fn assert_print_success(event: AgentEvent, command_id: &str) {
    match event.event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
            assert_eq!(result.error, "");
            assert_eq!(
                serde_json::from_str::<TestPrintProjectFileResult>(&result.result_json).unwrap(),
                TestPrintProjectFileResult {
                    kind: "print_project_file".to_owned(),
                    serial_number: "SERIAL1".to_owned(),
                    job_id: "job-1".to_owned(),
                    artifact_id: "artifact-1".to_owned(),
                    uploaded_path: "plate.gcode.3mf".to_owned(),
                    uploaded_url: "ftp://plate.gcode.3mf".to_owned(),
                    md5: "900150983CD24FB0D6963F7D28E17F72".to_owned(),
                    mqtt: TestPrintProjectMqttResult {
                        topic: "device/SERIAL1/request".to_owned(),
                        qos: 0,
                        payload: TestPrintProjectMqttPayload {
                            print: TestPrintProjectPrintPayload {
                                command: "project_file".to_owned(),
                            },
                        },
                    },
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

fn assert_failure_excludes(event: AgentEvent, command_id: &str, needles: &[&str]) {
    match event.event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(!result.success);
            for needle in needles {
                assert!(
                    !result.error.contains(needle),
                    "failure leaked {needle}: {}",
                    result.error
                );
            }
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

fn temp_artifact_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pandar-agent-test-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
