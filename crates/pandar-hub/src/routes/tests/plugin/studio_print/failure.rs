use pandar_core::{PrintTransferFailure, PrintTransferPhase};

use super::*;

#[tokio::test]
async fn failed_studio_transfer_returns_persisted_phase_and_redacted_job_cause() {
    let (state, app, tenant_id, agent_id, printer_id, token) =
        studio_fixture("studio-transfer-failure").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let job = state
        .jobs()
        .get_by_studio_submission_id(
            tenant_id,
            pandar_core::StudioSubmissionId::try_from(i64::from(created.studio_submission_id))
                .unwrap(),
        )
        .await
        .unwrap()
        .unwrap()
        .job;
    state
        .jobs()
        .mark_print_sent(job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_acknowledged(job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    let cause = "dispatch print job: start protected upload: 522 SSL connection failed: session reuse required [redacted]";
    let result_json = serde_json::to_string(&PrintTransferFailure {
        phase: PrintTransferPhase::DataConnection,
        cause: "result JSON cause is not the Studio source of truth".to_owned(),
    })
    .unwrap();
    state
        .jobs()
        .mark_print_failed_with_result(
            job.command_id,
            tenant_id,
            agent_id,
            cause.to_owned(),
            Some(result_json),
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/plugin/jobs/{}", created.studio_submission_id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["failure"],
        serde_json::json!({
            "phase": "data_connection",
            "cause": cause,
        })
    );
}

#[tokio::test]
async fn failed_studio_job_without_transfer_result_omits_failure_detail() {
    let (state, app, tenant_id, agent_id, printer_id, token) =
        studio_fixture("studio-generic-failure").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let job = state
        .jobs()
        .get_by_studio_submission_id(
            tenant_id,
            pandar_core::StudioSubmissionId::try_from(i64::from(created.studio_submission_id))
                .unwrap(),
        )
        .await
        .unwrap()
        .unwrap()
        .job;
    state
        .jobs()
        .mark_print_sent(job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_failed(
            job.command_id,
            tenant_id,
            agent_id,
            "generic dispatch failure".to_owned(),
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/plugin/jobs/{}", created.studio_submission_id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["job_status"], serde_json::json!("failed"));
    assert!(body.get("failure").is_none());
}
