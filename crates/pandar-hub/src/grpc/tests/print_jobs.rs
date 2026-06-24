use pandar_core::{AgentId, JobStatus, TenantId};
use tokio_stream::StreamExt;
use tonic::Code;

use super::*;
use crate::{
    db::Database,
    protocol::agent::v1::hub_command,
    repositories::{CreatePrintJob, test_helpers::insert_printer_fixture},
};

#[tokio::test]
async fn grpc_dispatch_print_project_file_sends_payload_and_marks_job_sent() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job(&state, tenant_id, agent_id, "artifact-1").await;
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let hub_command = stream.next().await.unwrap().unwrap();

    assert_eq!(hub_command.command_id, created.job.command_id.to_string());
    let Some(hub_command::Command::PrintProjectFile(print)) = hub_command.command else {
        panic!("expected print project file command");
    };
    assert_eq!(print.job_id, created.job.id.to_string());
    assert_eq!(print.artifact_id, created.artifact.id);
    assert_eq!(print.printer_id, created.job.printer_id);
    assert_eq!(print.filename, "plate.3mf");
    assert_eq!(print.storage_path, created.artifact.storage_path);
    assert_eq!(
        print.artifact_download_path,
        format!(
            "/api/v1/agents/{}/artifacts/{}",
            agent_id, created.artifact.id
        )
    );
    assert_eq!(print.size_bytes, 42);
    assert!(print.serial_number.starts_with("serial-"));
    assert_eq!(
        state
            .jobs()
            .get_for_tenant(tenant_id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Sent
    );
}

#[tokio::test]
async fn grpc_dispatch_print_project_file_sends_mapping_strings() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job_with_mappings(
        &state,
        tenant_id,
        agent_id,
        "artifact-1",
        Some("[0,254]".to_string()),
        Some(r#"[{"ams_id":254,"slot_id":1}]"#.to_string()),
    )
    .await;
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let hub_command = stream.next().await.unwrap().unwrap();

    assert_eq!(hub_command.command_id, created.job.command_id.to_string());
    let Some(hub_command::Command::PrintProjectFile(print)) = hub_command.command else {
        panic!("expected print project file command");
    };
    assert_eq!(print.ams_mapping_json, "[0,254]");
    assert_eq!(print.ams_mapping2_json, r#"[{"ams_id":254,"slot_id":1}]"#);
}

#[tokio::test]
async fn grpc_corrupt_persisted_mapping_streams_internal_error() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job_with_mappings(
        &state,
        tenant_id,
        agent_id,
        "artifact-1",
        Some("[0]".to_string()),
        None,
    )
    .await;
    corrupt_command_mapping(&state, created.job.command_id).await;
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::Internal);
    assert_eq!(err.message(), "invalid print command mapping payload");
    assert_eq!(
        state
            .jobs()
            .get_for_tenant(tenant_id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Queued
    );
}

#[tokio::test]
async fn grpc_print_ack_and_result_update_linked_job() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job(&state, tenant_id, agent_id, "artifact-1").await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(ack_event(tenant_id, agent_id, created.job.command_id)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        state
            .jobs()
            .get_for_tenant(tenant_id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Acknowledged
    );

    sender
        .send(Ok(success_event(
            tenant_id,
            agent_id,
            created.job.command_id,
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        state
            .jobs()
            .get_for_tenant(tenant_id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Succeeded
    );
}

#[tokio::test]
async fn grpc_stale_print_result_does_not_update_job() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job(&state, tenant_id, agent_id, "artifact-1").await;
    let command_id = created.job.command_id;
    state
        .commands()
        .mark_sent(command_id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_failed(command_id, tenant_id, agent_id, "first")
        .await
        .unwrap();
    state
        .jobs()
        .mark_for_command(command_id, JobStatus::Failed, Some("first".to_string()))
        .await
        .unwrap();
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(success_event(tenant_id, agent_id, command_id)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
    let job = state
        .jobs()
        .get_for_tenant(tenant_id, created.job.id)
        .await
        .unwrap()
        .unwrap()
        .job;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.as_deref(), Some("first"));
}

#[tokio::test]
async fn grpc_malformed_print_payload_streams_internal_error() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let created = create_print_job(&state, tenant_id, agent_id, "artifact-1").await;
    corrupt_command_payload(&state, created.job.command_id).await;
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::Internal);
    assert_eq!(
        state
            .jobs()
            .get_for_tenant(tenant_id, created.job.id)
            .await
            .unwrap()
            .unwrap()
            .job
            .status,
        JobStatus::Queued
    );
}

async fn create_print_job(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    artifact_id: &str,
) -> crate::repositories::JobWithArtifact {
    create_print_job_with_mappings(state, tenant_id, agent_id, artifact_id, None, None).await
}

async fn create_print_job_with_mappings(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    artifact_id: &str,
    ams_mapping_json: Option<String>,
    ams_mapping2_json: Option<String>,
) -> crate::repositories::JobWithArtifact {
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id,
            printer_id: printer_id.clone(),
            agent_id,
            artifact_id: artifact_id.to_string(),
            artifact_filename: "plate.3mf".to_string(),
            artifact_content_type: "model/3mf".to_string(),
            artifact_size_bytes: 42,
            artifact_storage_path: format!("{tenant_id}/{artifact_id}/plate.3mf"),
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: true,
            ams_mapping_json,
            ams_mapping2_json,
        })
        .await
        .unwrap()
}

async fn corrupt_command_payload(state: &AppState, command_id: pandar_core::CommandId) {
    match state.database() {
        Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET payload_json = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind("{")
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET payload_json = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind("{")
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

async fn corrupt_command_mapping(state: &AppState, command_id: pandar_core::CommandId) {
    let payload = r#"{"job_id":"job","artifact_id":"artifact","printer_id":"printer","serial_number":"serial","filename":"plate.3mf","storage_path":"tenant/artifact/plate.3mf","artifact_download_path":"/api/v1/agents/agent/artifacts/artifact","size_bytes":42,"plate_id":1,"use_ams":true,"flow_cali":false,"timelapse":true,"ams_mapping_json":"[{}]","ams_mapping2_json":null}"#;
    match state.database() {
        Database::Sqlite(pool) => {
            sqlx::query("UPDATE commands SET payload_json = ?2 WHERE id = ?1")
                .bind(command_id.to_string())
                .bind(payload)
                .execute(pool)
                .await
                .unwrap();
        }
        Database::Postgres(pool) => {
            sqlx::query("UPDATE commands SET payload_json = $2 WHERE id = $1")
                .bind(command_id.to_string())
                .bind(payload)
                .execute(pool)
                .await
                .unwrap();
        }
    }
}
