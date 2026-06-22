use pandar_core::{AgentId, PrintStatus, TenantId};
use tonic::Code;

use super::*;
use crate::{
    protocol::agent::v1::{MachineDiagnostic, PrintJobReport},
    repositories::{CreatePrintJob, test_helpers::insert_printer_fixture},
};

#[tokio::test]
async fn grpc_print_job_report_updates_job_print_state() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();

    sender
        .send(Ok(report_event(
            tenant_id,
            agent_id,
            report(serial, created.job.id.to_string(), created.artifact.id),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let job = state
        .jobs()
        .get_for_tenant(tenant_id, created.job.id)
        .await
        .unwrap()
        .unwrap()
        .job;
    assert_eq!(job.print.status, PrintStatus::Running);
    assert_eq!(job.print.progress_percent, Some(57));
    assert_eq!(job.print.remaining_time_minutes, Some(31));
    assert_eq!(job.print.current_layer, Some(4));
    assert_eq!(job.print.total_layers, Some(12));
    assert_eq!(job.print.error, None);
}

#[tokio::test]
async fn grpc_print_job_report_rejects_invalid_observed_at() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();
    let mut report = report(serial, created.job.id.to_string(), created.artifact.id);
    report.observed_at = "not-a-date".to_string();

    sender
        .send(Ok(report_event(tenant_id, agent_id, report)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn grpc_print_job_report_rejects_invalid_artifact_id() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();
    let report = report(serial, created.job.id.to_string(), "not-a-uuid".to_string());

    sender
        .send(Ok(report_event(tenant_id, agent_id, report)))
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();

    assert_eq!(err.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn grpc_print_job_report_drops_out_of_range_metrics() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let mut report = report(serial, created.job.id.to_string(), created.artifact.id);
    report.percent = 101;
    report.remaining_time_minutes = 4321;
    report.current_layer = 100_001;
    report.total_layers = 100_001;

    sender
        .send(Ok(report_event(tenant_id, agent_id, report)))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let job = state
        .jobs()
        .get_for_tenant(tenant_id, created.job.id)
        .await
        .unwrap()
        .unwrap()
        .job;
    assert_eq!(job.print.status, PrintStatus::Running);
    assert_eq!(job.print.progress_percent, None);
    assert_eq!(job.print.remaining_time_minutes, None);
    assert_eq!(job.print.current_layer, None);
    assert_eq!(job.print.total_layers, None);
}

async fn create_print_job(
    state: &AppState,
    tenant_id: TenantId,
    agent_id: AgentId,
    artifact_id: &str,
) -> (crate::repositories::JobWithArtifact, String) {
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let created = state
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
        })
        .await
        .unwrap();
    (created, format!("serial-{printer_id}"))
}

fn report(serial: String, job_id: String, artifact_id: String) -> PrintJobReport {
    PrintJobReport {
        serial,
        job_id,
        artifact_id,
        subtask_id: String::new(),
        gcode_file: "plate.3mf".to_string(),
        subtask_name: String::new(),
        gcode_state: "RUNNING".to_string(),
        percent: 57,
        has_percent: true,
        remaining_time_minutes: 31,
        has_remaining_time_minutes: true,
        current_layer: 4,
        has_current_layer: true,
        total_layers: 12,
        has_total_layers: true,
        diagnostics: vec![MachineDiagnostic {
            kind: "hms".to_string(),
            severity: "warning".to_string(),
            code: "HMS_123".to_string(),
            message: "fan warning".to_string(),
            payload_json: r#"{"code":"HMS_123"}"#.to_string(),
        }],
        observed_at: "2026-06-22T10:00:00Z".to_string(),
    }
}

fn report_event(tenant_id: TenantId, agent_id: AgentId, report: PrintJobReport) -> AgentEvent {
    AgentEvent {
        tenant_id: tenant_id.to_string(),
        agent_id: agent_id.to_string(),
        event_id: "event".to_string(),
        event: Some(agent_event::Event::PrintJobReport(report)),
    }
}

const ARTIFACT_ID: &str = "11111111-1111-4111-8111-111111111111";
