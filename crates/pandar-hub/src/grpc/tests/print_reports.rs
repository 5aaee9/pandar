use pandar_core::{AgentId, PrintStatus, TenantId};
use tonic::Code;

use super::*;
use crate::{
    printer_events::PrinterEvent,
    protocol::agent::v1::{MachineDiagnostic, PrintJobReport, PrinterHmsItem},
    repositories::{CreatePrintJob, test_helpers::insert_printer_fixture},
};

mod live_status;

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
async fn grpc_print_job_report_ignores_non_pandar_artifact_id() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let (_stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let report = report(serial, created.job.id.to_string(), "not-a-uuid".to_string());

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
    assert_eq!(job.print.progress_percent, Some(57));
}

#[tokio::test]
async fn grpc_print_job_report_ignores_non_pandar_job_id() {
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
            report(serial, "0".to_string(), created.artifact.id),
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

#[tokio::test]
async fn replacement_session_blocks_old_print_report_commit() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let (created, serial) = create_print_job(&state, tenant_id, agent_id, ARTIFACT_ID).await;
    let initial_status = created.job.print.status;
    let initial_progress = created.job.print.progress_percent;
    let job_id = created.job.id;
    let artifact_id = created.artifact.id.clone();
    let (_old_stream, _old_sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let old_token = state.sessions().get(agent_id).await.unwrap().token;
    let mut paused = crate::sessions::transition_pause::install_before(old_token);

    let old_state = state.clone();
    let old_report = tokio::spawn(async move {
        handle_event(
            &old_state,
            tenant_id,
            agent_id,
            old_token,
            report_event(
                tenant_id,
                agent_id,
                report(serial, job_id.to_string(), artifact_id),
            ),
        )
        .await
    });
    paused.wait_until_reached().await;

    let replacement = register_test_session(&state, tenant_id, agent_id).await;
    paused.resume();
    old_report.await.unwrap().unwrap();

    let job = state
        .jobs()
        .get_for_tenant(tenant_id, job_id)
        .await
        .unwrap()
        .unwrap()
        .job;
    assert_eq!(job.print.status, initial_status);
    assert_eq!(job.print.progress_percent, initial_progress);
    let persisted = persisted_agent(&state, agent_id).await;
    assert_eq!(
        persisted.current_session_id,
        Some(replacement.persisted_id())
    );
}

#[tokio::test]
async fn live_and_material_print_report_coalesces_one_enriched_printer_event() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();
    let _control_plane = start_control_plane(state.clone()).await;
    let printer_id = state
        .printers()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        PrintJobReport {
            serial: "serial".to_owned(),
            observed_at: "2026-07-02T00:00:00Z".to_owned(),
            printer_materials_json: crate::grpc::tests::printer_snapshots::valid_material_patch(
                "2026-07-02T00:00:00Z",
            ),
            ..report("serial".to_owned(), String::new(), String::new())
        },
    )
    .await
    .unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
    assert_eq!(printer.id, printer_id);
    assert!(printer.materials.is_some());
    assert!(printer.state_revision.is_some());
    let print = printer.print.as_ref().expect("enriched print state");
    assert_eq!(print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(print.progress_percent, Some(57));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await
            .is_err(),
        "report must emit one snapshot"
    );
}

#[tokio::test]
async fn uncorrelated_live_print_report_publishes_enriched_printer_event() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();
    let _control_plane = start_control_plane(state.clone()).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        report("serial".to_owned(), String::new(), String::new()),
    )
    .await
    .unwrap();

    let event = receiver.recv().await.unwrap();
    let PrinterEvent::PrinterSnapshot { printer } = event else {
        panic!("expected printer snapshot")
    };
    assert!(printer.state_revision.is_some());
    let print = printer.print.expect("enriched print state");
    assert_eq!(print.task_generation, 1);
    assert_eq!(print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(print.progress_percent, Some(57));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await
            .is_err(),
        "uncorrelated report must emit only one snapshot"
    );
}

#[tokio::test]
async fn last_seen_only_print_report_advances_revision_without_event() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();
    let _control_plane = start_control_plane(state.clone()).await;
    let mut receiver = state.printer_events().subscribe(tenant_id).await;
    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        report("serial".to_owned(), String::new(), String::new()),
    )
    .await
    .unwrap();
    receiver.recv().await.unwrap();
    let before = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .state_revision;

    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        PrintJobReport {
            serial: "serial".to_owned(),
            observed_at: "2026-06-22T10:01:00Z".to_owned(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let after = state
        .printers()
        .list_with_live_status_for_tenant(tenant_id)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .state_revision;
    assert_eq!(after, before + 1);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await
            .is_err(),
        "last-seen-only report must emit no snapshot"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_print_report_material_patch_is_logged_and_dropped() {
    let logs = super::log_capture::CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let token = register_test_session(&state, tenant_id, agent_id).await;
    handle_snapshot(
        &state,
        tenant_id,
        agent_id,
        token,
        crate::grpc::tests::printer_snapshots::snapshot("serial", "Printer", "A1", "IDLE"),
    )
    .await
    .unwrap();

    let _guard = tracing::subscriber::set_default(subscriber);
    handle_print_report(
        &state,
        tenant_id,
        agent_id,
        token,
        PrintJobReport {
            serial: "serial".to_owned(),
            observed_at: "2026-07-02T00:00:00Z".to_owned(),
            printer_materials_json:
                r#"{"type":"printer_material_patch","observed_at":"bad","password":"secret"}"#
                    .to_owned(),
            ..report("serial".to_owned(), String::new(), String::new())
        },
    )
    .await
    .unwrap();
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("ignored print report material patch"));
    assert!(captured.contains("invalid material patch JSON"));
    assert!(!captured.contains("secret"));
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
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            flow_cali: false,
            timelapse: true,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
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
        hms: vec![PrinterHmsItem {
            attr: 0x0102_0304,
            code: 0x0506_0708,
        }],
        has_hms: true,
        diagnostics: vec![MachineDiagnostic {
            kind: "hms".to_string(),
            severity: "warning".to_string(),
            code: "HMS_123".to_string(),
            message: "fan warning".to_string(),
            payload_json: r#"{"code":"HMS_123"}"#.to_string(),
        }],
        printer_materials_json: String::new(),
        observed_at: "2026-06-22T10:00:00Z".to_string(),
        ..Default::default()
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
