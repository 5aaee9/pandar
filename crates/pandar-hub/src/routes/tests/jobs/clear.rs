use super::*;
use crate::repositories::{ApplyPrintReport, CreatePrintJob, UserRole};

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ClearJobsResponse {
    deleted_jobs: u64,
    retained_jobs: u64,
    deleted_commands: u64,
    deleted_artifacts: u64,
    deleted_artifact_bytes: u64,
}

#[tokio::test]
async fn tenant_admin_clears_only_terminal_jobs_and_is_idempotent() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("clear-route", "Clear Route")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let terminal = create_job(&state, tenant.id, agent.id, &printer_id, "terminal").await;
    state
        .jobs()
        .mark_print_sent(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_succeeded(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .apply_print_report(report(
            tenant.id,
            agent.id,
            &printer_id,
            terminal.job.id,
            "FINISH",
        ))
        .await
        .unwrap();
    let active = create_job(&state, tenant.id, agent.id, &printer_id, "active").await;
    let operator =
        external_auth_token_for_role(&state, tenant.id, UserRole::Operator, "clear-operator").await;
    let admin =
        external_auth_token_for_role(&state, tenant.id, UserRole::TenantAdmin, "clear-admin").await;
    let uri = format!("/api/v1/tenants/{}/jobs", tenant.id);

    let (status, body) = request_as(app.clone(), Method::DELETE, &uri, None, &operator).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let (status, body) = request_as(app.clone(), Method::DELETE, &uri, None, &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode::<ClearJobsResponse>(body),
        ClearJobsResponse {
            deleted_jobs: 1,
            retained_jobs: 1,
            deleted_commands: 1,
            deleted_artifacts: 1,
            deleted_artifact_bytes: 42,
        }
    );
    assert!(
        state
            .jobs()
            .get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .jobs()
            .get_for_tenant(tenant.id, active.job.id)
            .await
            .unwrap()
            .is_some()
    );

    let (status, body) = request_as(app, Method::DELETE, &uri, None, &admin).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<ClearJobsResponse>(body).deleted_jobs, 0);
    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.action == "job.clear")
            .count(),
        2
    );
    assert!(events.iter().any(|event| {
        event.action == "job.clear"
            && event.actor_type == "user"
            && event.user_id.is_some()
            && event.metadata_json.contains("\"deleted_jobs\":1")
    }));
}

#[tokio::test]
async fn no_auth_can_clear_jobs_and_records_no_auth_actor() {
    let state = state().await.with_no_auth_for_tests(true);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("clear-no-auth", "Clear No Auth")
        .await
        .unwrap();

    let (status, body) = request(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{}/jobs", tenant.id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<ClearJobsResponse>(body).deleted_jobs, 0);
    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.clear")
        .unwrap();
    assert_eq!(event.actor_type, "no_auth");
    assert_eq!(event.user_id, None);
}

#[tokio::test]
async fn all_scope_tenant_token_can_clear_jobs() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("clear-token", "Clear Token")
        .await
        .unwrap();
    let token = all_scope_tenant_token(&state, &tenant.id.to_string(), "clear-all-scope").await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{}/jobs", tenant.id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<ClearJobsResponse>(body).deleted_jobs, 0);
    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.clear")
        .unwrap();
    assert_eq!(event.actor_type, "tenant_token");
}

pub(super) async fn create_job(
    state: &AppState,
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    printer_id: &str,
    artifact_id: &str,
) -> crate::repositories::JobWithArtifact {
    state
        .jobs()
        .create_print_job(CreatePrintJob {
            tenant_id,
            printer_id: printer_id.to_owned(),
            agent_id,
            artifact_id: artifact_id.to_owned(),
            artifact_filename: "plate.3mf".to_owned(),
            artifact_content_type: "model/3mf".to_owned(),
            artifact_size_bytes: 42,
            artifact_storage_path: format!("{tenant_id}/{artifact_id}/plate.3mf"),
            artifact_metadata_json: None,
            plate_id: 1,
            use_ams: true,
            bed_leveling: false,
            auto_bed_leveling: pandar_core::PrintCalibrationMode::Off,
            flow_cali: false,
            auto_flow_cali: pandar_core::PrintCalibrationMode::Off,
            auto_offset_cali: pandar_core::PrintCalibrationMode::Off,
            timelapse: false,
            ams_mapping_json: None,
            ams_mapping2_json: None,
            ams_mapping_info_json: None,
        })
        .await
        .unwrap()
}

pub(super) fn report(
    tenant_id: pandar_core::TenantId,
    agent_id: pandar_core::AgentId,
    printer_id: &str,
    job_id: pandar_core::JobId,
    gcode_state: &str,
) -> ApplyPrintReport {
    ApplyPrintReport {
        tenant_id,
        agent_id,
        serial: format!("serial-{printer_id}"),
        task_id: Some(job_id.to_string()),
        job_id: Some(job_id),
        print_error: None,
        printer_job_id: None,
        job_attr: None,
        artifact_id: None,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: Some(gcode_state.to_owned()),
        percent: Some(100),
        remaining_time_minutes: Some(0),
        current_layer: Some(10),
        total_layers: Some(10),
        hms: None,
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-07-15T00:00:00Z".to_owned(),
    }
}
