use anyhow::Context;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    AgentConfig,
    machine::BambuMachineGateway,
    protocol::agent::v1::{AgentEvent, PrintProjectFile},
};

use super::{
    ArtifactReader,
    artifacts::{CommandArtifactReader, LegacyCommandArtifactReader, PrintCommandArtifactReader},
    responses::{ack_event, failure_event, rejected_ack_event, success_event_with_result},
};

pub(super) async fn emit_print_project_file_events<G>(
    config: &AgentConfig,
    gateway: &G,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: PrintProjectFile,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
{
    let artifact_reader = CommandArtifactReader::new(config);
    emit_print_project_file_events_with_command_reader(
        config,
        gateway,
        &artifact_reader,
        sender,
        command_id,
        command,
    )
    .await
}

pub(super) async fn emit_print_project_file_events_with_reader<G, R>(
    config: &AgentConfig,
    gateway: &G,
    artifact_reader: &R,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: PrintProjectFile,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    R: ArtifactReader,
{
    emit_print_project_file_events_with_command_reader(
        config,
        gateway,
        &LegacyCommandArtifactReader { artifact_reader },
        sender,
        command_id,
        command,
    )
    .await
}

pub(super) async fn emit_print_project_file_events_with_command_reader<G, R>(
    config: &AgentConfig,
    gateway: &G,
    artifact_reader: &R,
    sender: &mpsc::Sender<AgentEvent>,
    command_id: &str,
    command: PrintProjectFile,
) -> anyhow::Result<()>
where
    G: BambuMachineGateway,
    R: PrintCommandArtifactReader,
{
    if let Err(err) = gateway.validate_printer(&command.serial_number).await {
        let error = gateway.redact_error(&format!("{err:#}"));
        sender
            .send(rejected_ack_event(config, command_id, error))
            .await
            .context("queue print-project-file rejected ack")?;
        return Ok(());
    }

    sender
        .send(ack_event(config, command_id))
        .await
        .context("queue print-project-file command ack")?;

    let result = async {
        let artifact = artifact_reader
            .read_print_artifact(&command)
            .await
            .with_context(|| read_print_artifact_context(&command))?;
        gateway
            .print_project_file(&command.serial_number, &command, artifact)
            .await
            .with_context(|| format!("dispatch print job {}", command.job_id))
    }
    .await;

    match result {
        Ok(dispatch) => {
            let result_json = serde_json::to_string(&PrintProjectFileResult {
                kind: "print_project_file",
                serial_number: &command.serial_number,
                job_id: &command.job_id,
                artifact_id: &command.artifact_id,
                uploaded_path: &dispatch.uploaded_path,
                uploaded_url: &dispatch.uploaded_url,
                md5: &dispatch.md5,
                mqtt: PrintProjectMqttResult {
                    topic: &dispatch.topic,
                    qos: dispatch.qos,
                    payload: &dispatch.payload,
                },
            })
            .expect("print-project-file result is serializable");
            sender
                .send(success_event_with_result(config, command_id, result_json))
                .await
                .context("queue print-project-file command success")?;
        }
        Err(err) => {
            let error = gateway.redact_error(&format!("{err:#}"));
            sender
                .send(failure_event(config, command_id, error))
                .await
                .context("queue print-project-file command failure")?;
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct PrintProjectFileResult<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    serial_number: &'a str,
    job_id: &'a str,
    artifact_id: &'a str,
    uploaded_path: &'a str,
    uploaded_url: &'a str,
    md5: &'a str,
    mqtt: PrintProjectMqttResult<'a>,
}

#[derive(Serialize)]
struct PrintProjectMqttResult<'a> {
    topic: &'a str,
    qos: u8,
    payload: &'a Value,
}

fn read_print_artifact_context(command: &PrintProjectFile) -> String {
    if command.artifact_download_path.trim().is_empty() {
        format!("read print artifact {}", command.storage_path)
    } else {
        "read print artifact from hub".to_string()
    }
}
