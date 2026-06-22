use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};

use super::{assert_failure_contains, test_config};
use crate::{
    commands::{
        ArtifactReader, FilesystemArtifactReader, ack_event, handle_command_with_reader,
        success_event,
    },
    machine::{BambuMachineGateway, MachineSnapshot},
    protocol::agent::v1::{AgentEvent, HubCommand, PrintProjectFile, agent_event, hub_command},
};

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
    assert_eq!(
        receiver.recv().await.unwrap(),
        success_event(&config, &command_id)
    );
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
    assert_eq!(
        receiver.recv().await.unwrap(),
        success_event(&config, &command_id)
    );
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
            size_bytes: 3,
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: true,
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

fn temp_artifact_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pandar-agent-test-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[derive(Debug, Clone, Default)]
struct FakeArtifactReader {
    artifacts: Arc<HashMap<String, Vec<u8>>>,
    reads: Arc<Mutex<Vec<String>>>,
}

impl FakeArtifactReader {
    fn with_artifacts(artifacts: impl IntoIterator<Item = (&'static str, Vec<u8>)>) -> Self {
        Self {
            artifacts: Arc::new(
                artifacts
                    .into_iter()
                    .map(|(path, bytes)| (path.to_string(), bytes))
                    .collect(),
            ),
            reads: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ArtifactReader for FakeArtifactReader {
    async fn read_artifact(&self, storage_path: &str) -> anyhow::Result<Vec<u8>> {
        self.reads.lock().await.push(storage_path.to_string());
        crate::commands::resolve_artifact_path(std::path::Path::new("."), storage_path)?;
        self.artifacts
            .get(storage_path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("fake artifact missing at {storage_path}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedPrint {
    serial_number: String,
    job_id: String,
    artifact: Vec<u8>,
}

#[derive(Debug, Clone)]
struct FakePrintGateway {
    prints: Arc<Mutex<Vec<RecordedPrint>>>,
    valid_serials: Vec<String>,
}

impl FakePrintGateway {
    fn ok(serials: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            prints: Arc::new(Mutex::new(Vec::new())),
            valid_serials: serials.into_iter().map(str::to_string).collect(),
        }
    }
}

#[async_trait]
impl BambuMachineGateway for FakePrintGateway {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        Ok(Vec::new())
    }

    async fn validate_printer(&self, serial_number: &str) -> anyhow::Result<()> {
        if self
            .valid_serials
            .iter()
            .any(|serial| serial == serial_number)
        {
            return Ok(());
        }

        anyhow::bail!("no configured Bambu printer matches serial {serial_number}")
    }

    async fn print_project_file(
        &self,
        serial_number: &str,
        command: &PrintProjectFile,
        artifact: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.prints.lock().await.push(RecordedPrint {
            serial_number: serial_number.to_string(),
            job_id: command.job_id.clone(),
            artifact,
        });
        Ok(())
    }
}
