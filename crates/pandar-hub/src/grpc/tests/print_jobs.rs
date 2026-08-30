use pandar_core::{AgentId, JobStatus, PrintStatus, TenantId};
use tokio_stream::StreamExt;
use tonic::Code;

use super::*;
use crate::{
    db::Database,
    repositories::{
        CreatePrintJob, PrinterOperationKind,
        test_helpers::{insert_printer_fixture, insert_printer_fixture_with_model},
    },
};
use pandar_protocol::agent::v1::hub_command;

mod print_failures;
mod support;
use support::*;

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
        print.studio_submission_id,
        created.job.studio_submission_id.get() as u32
    );
    let options = print.options.expect("typed Studio print options");
    assert!(options.bed_leveling);
    assert!(!options.flow_cali);
    assert_eq!(options.auto_bed_leveling, Some(2));
    assert_eq!(options.auto_flow_cali, Some(1));
    assert_eq!(options.auto_offset_cali, Some(0));
    assert_eq!(options.extruder_cali_manual_mode, None);
    assert!(options.try_emmc_print);
    assert_eq!(
        print.submission_source,
        pandar_protocol::agent::v1::PrintSubmissionSource::Web as i32
    );
    assert!(print.task_metadata.is_none());
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
async fn grpc_dispatch_studio_print_projects_exact_metadata_and_source() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let mut metadata = crate::test_support::studio_metadata_for_tests();
    let pandar_core::StudioPrintMetadata::V1(studio) = &mut metadata;
    studio.task_name = "Studio exact task".to_owned();
    studio.task_bed_type = "pei".to_owned();
    studio.extruder_cali_manual_mode = -1;
    studio.try_emmc_print = false;
    studio.task_bed_leveling = false;
    studio.auto_bed_leveling = pandar_core::PrintCalibrationMode::Off;
    let created = state
        .jobs()
        .create_studio_print_job_with_audit(
            print_input(tenant_id, agent_id, &printer_id, "studio-exact", None, None),
            metadata,
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    let (mut stream, _sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    let hub_command = stream.next().await.unwrap().unwrap();
    let Some(hub_command::Command::PrintProjectFile(print)) = hub_command.command else {
        panic!("expected print project file command");
    };
    assert_eq!(
        print.submission_source,
        pandar_protocol::agent::v1::PrintSubmissionSource::Studio as i32
    );
    assert_eq!(
        print.studio_submission_id,
        created.job.studio_submission_id.get() as u32
    );
    let options = print.options.unwrap();
    assert!(!options.bed_leveling);
    assert_eq!(options.auto_bed_leveling, Some(0));
    assert_eq!(options.bed_type, "pei");
    assert_eq!(options.extruder_cali_manual_mode, Some(-1));
    assert!(!options.try_emmc_print);
    assert_eq!(print.task_metadata.unwrap().task_name, "Studio exact task");
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
    let options = print.options.expect("typed Studio print options");
    assert_eq!(options.ams_mapping, vec![0, 254]);
    assert_eq!(options.ams_mapping2.len(), 1);
    assert_eq!(options.ams_mapping2[0].ams_id, 254);
    assert_eq!(options.ams_mapping2[0].slot_id, 1);
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
    assert_eq!(err.message(), "invalid print command payload");
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

#[tokio::test]
async fn printer_operation_success_does_not_mutate_physical_print_status() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let printer_id =
        insert_printer_fixture_with_model(state.database(), tenant_id, agent_id, Some("A1 Mini"))
            .await
            .unwrap();
    let created = state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id,
            printer_id: printer_id.clone(),
            agent_id,
            artifact: crate::repositories::PrintArtifactInput {
                id: "artifact-1".to_string(),
                filename: "plate.3mf".to_string(),
                content_type: "model/3mf".to_string(),
                size_bytes: 42,
                storage_path: format!("{tenant_id}/artifact-1/plate.3mf"),
                metadata_json: None,
            },
            options: crate::repositories::PrintExecutionOptions {
                plate_id: 1,
                use_ams: true,
                auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
                bed_leveling: false,
                flow_cali: false,
                auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
                auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
                timelapse: false,
                ams_mapping_json: None,
                ams_mapping2_json: None,
                ams_mapping_info_json: None,
            },
        })
        .await
        .unwrap();
    let control = state
        .commands()
        .enqueue_printer_operation_with_audit(
            tenant_id,
            &printer_id,
            PrinterOperationKind::Stop {},
            test_audit_actor(),
        )
        .await
        .unwrap();

    state
        .commands()
        .mark_sent(control.id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_acknowledged(control.id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .commands()
        .mark_succeeded_with_result(
            control.id,
            tenant_id,
            agent_id,
            Some(r#"{"type":"printer_operation","action":"stop"}"#.to_string()),
        )
        .await
        .unwrap();

    let reloaded = state
        .jobs()
        .get_for_tenant(tenant_id, created.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.job.print.status, PrintStatus::Pending);
}
