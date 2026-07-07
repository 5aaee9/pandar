use pandar_core::{AgentId, CommandId, JobId};

use super::*;

#[tokio::test]
async fn job_recovery_routes_retry_reprint_duplicate_and_audit() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "recovery-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();

    let (_, retry_source) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    let retry_source = decode::<JobResponse>(retry_source);
    let retry_job_id = retry_source.id.clone();
    let retry_command_id = retry_source.command_id.clone();
    state
        .jobs()
        .mark_print_sent(
            CommandId::parse(&retry_command_id).unwrap(),
            tenant_id,
            agent.id,
        )
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_failed(
            CommandId::parse(&retry_command_id).unwrap(),
            tenant_id,
            agent.id,
            "agent offline".to_owned(),
        )
        .await
        .unwrap();

    let (status, retried) = request_as(
        app.clone(),
        Method::POST,
        &format!(
            "/api/v1/tenants/{tenant_id}/jobs/{}/retry-dispatch",
            retry_job_id
        ),
        Some(json!({ "reason": "operator retry" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let retried = decode::<JobResponse>(retried);
    assert_eq!(retried.id, retry_job_id);
    assert_eq!(retried.status, "queued");
    assert_ne!(retried.command_id, retry_command_id);

    let (_, finished) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    let finished = decode::<JobResponse>(finished);
    let finished_job_id = JobId::parse(&finished.id).unwrap();
    state
        .jobs()
        .apply_print_report(report_input(
            tenant_id,
            agent.id,
            &printer_id,
            Some(finished_job_id),
            None,
            "FINISH",
        ))
        .await
        .unwrap();

    let (status, reprint) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{finished_job_id}/reprint"),
        Some(json!({ "reason": "print another" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let reprint = decode::<JobResponse>(reprint);
    assert_ne!(reprint.id, finished_job_id.to_string());
    assert_eq!(reprint.artifact.id, finished.artifact.id);

    let (status, duplicate) = request_as(
        app.clone(),
        Method::POST,
        &format!(
            "/api/v1/tenants/{tenant_id}/jobs/{}/duplicate",
            retry_job_id
        ),
        Some(json!({
            "printer_id": printer_id,
            "plate_id": 2,
            "use_ams": true,
            "flow_cali": true,
            "timelapse": false,
            "ams_mapping": null,
            "ams_mapping2": null
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let duplicate = decode::<JobResponse>(duplicate);
    assert_ne!(duplicate.id, retry_job_id);
    assert_eq!(duplicate.artifact.id, retry_source.artifact.id);

    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let retry_event = events
        .iter()
        .find(|event| event.action == "job.retry_dispatch")
        .unwrap();
    let retry_metadata: JobRecoveryAuditMetadata =
        serde_json::from_str(&retry_event.metadata_json).unwrap();
    assert_eq!(retry_metadata.source_job_id, retry_job_id);
    assert_eq!(retry_metadata.target_job_id, retry_job_id);
    assert_eq!(retry_metadata.source_command_id, retry_command_id);
    assert_eq!(retry_metadata.target_command_id, retried.command_id);
    assert_eq!(retry_metadata.reason.as_deref(), Some("operator retry"));

    let reprint_event = events
        .iter()
        .find(|event| event.action == "job.reprint")
        .unwrap();
    let reprint_metadata: JobRecoveryAuditMetadata =
        serde_json::from_str(&reprint_event.metadata_json).unwrap();
    assert_eq!(reprint_metadata.source_job_id, finished_job_id.to_string());
    assert_eq!(reprint_metadata.target_job_id, reprint.id);
    assert_eq!(reprint_metadata.source_command_id, finished.command_id);
    assert_eq!(reprint_metadata.target_command_id, reprint.command_id);

    let duplicate_event = events
        .iter()
        .find(|event| event.action == "job.duplicate")
        .unwrap();
    let duplicate_metadata: JobRecoveryAuditMetadata =
        serde_json::from_str(&duplicate_event.metadata_json).unwrap();
    assert_eq!(duplicate_metadata.source_job_id, retry_job_id);
    assert_eq!(duplicate_metadata.target_job_id, duplicate.id);
    assert_eq!(duplicate_metadata.source_command_id, retried.command_id);
    assert_eq!(duplicate_metadata.target_command_id, duplicate.command_id);
}

#[tokio::test]
async fn retry_dispatch_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "sibling-retry-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    let created = decode::<JobResponse>(created);
    let job_id = created.id;
    let command_id = CommandId::parse(&created.command_id).unwrap();
    state
        .jobs()
        .mark_print_sent(command_id, tenant_id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_failed(command_id, tenant_id, agent.id, "agent offline".to_owned())
        .await
        .unwrap();
    let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _) = tokio::sync::mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: agent.id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{}/retry-dispatch", job_id),
        Some(json!({ "reason": "operator retry" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(decode::<JobResponse>(body).status, "queued");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn reprint_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "sibling-reprint-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    let created = decode::<JobResponse>(created);
    let job_id = JobId::parse(&created.id).unwrap();
    state
        .jobs()
        .apply_print_report(report_input(
            tenant_id,
            agent.id,
            &printer_id,
            Some(job_id),
            None,
            "FINISH",
        ))
        .await
        .unwrap();
    let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _) = tokio::sync::mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: agent.id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint"),
        Some(json!({ "reason": "print another" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(decode::<JobResponse>(body).id, job_id.to_string());
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn duplicate_and_print_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "sibling-duplicate-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    let job_id = decode::<JobResponse>(created).id;
    let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _) = tokio::sync::mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: agent.id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{}/duplicate", job_id),
        Some(json!({
            "printer_id": printer_id,
            "plate_id": 2,
            "use_ams": true,
            "flow_cali": true,
            "timelapse": false,
            "ams_mapping": null,
            "ams_mapping2": null
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(decode::<JobResponse>(body).id, job_id);
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn job_recovery_routes_reject_unsafe_retry_and_viewer_auth() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let operator = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "unsafe-recovery-operator",
    )
    .await;
    let viewer = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Viewer,
        "unsafe-recovery-viewer",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &operator,
    )
    .await;
    let job_id = decode::<JobResponse>(created).id;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{}/retry-dispatch", job_id),
        Some(json!({ "reason": null })),
        &operator,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode::<ErrorResponse>(body).error, "retry_not_safe");

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{}/duplicate", job_id),
        Some(json!({})),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

fn report_input(
    tenant_id: TenantId,
    agent_id: AgentId,
    printer_id: &str,
    job_id: Option<JobId>,
    artifact_id: Option<String>,
    gcode_state: &str,
) -> crate::repositories::ApplyPrintReport {
    crate::repositories::ApplyPrintReport {
        tenant_id,
        agent_id,
        serial: format!("serial-{printer_id}"),
        job_id,
        artifact_id,
        subtask_id: None,
        gcode_file: None,
        subtask_name: None,
        gcode_state: Some(gcode_state.to_string()),
        percent: Some(42),
        remaining_time_minutes: Some(60),
        current_layer: Some(3),
        total_layers: Some(9),
        diagnostics: Vec::new(),
        printer_materials_json: String::new(),
        observed_at: "2026-06-22T00:00:00Z".to_string(),
    }
}
