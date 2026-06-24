use super::*;

#[tokio::test]
async fn linked_operator_jwt_can_create_print_job() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "linked-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printers/{printer_id}/jobs", tenant.id),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "queued");
}

#[tokio::test]
async fn linked_viewer_jwt_cannot_create_print_job() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "linked-job-viewer",
    )
    .await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printers/{printer_id}/jobs", tenant.id),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn job_create_rejects_invalid_tenant_printer_and_job_ids() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Operator,
        "invalid-job-operator",
    )
    .await;
    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/tenants/not-a-uuid/printers/printer/jobs",
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        "unused",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_tenant_id" }));

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/tenants/00000000-0000-0000-0000-000000000001/printers/not-a-uuid/jobs",
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));

    let (status, body) = request_as(
        app,
        Method::GET,
        "/api/v1/tenants/00000000-0000-0000-0000-000000000001/jobs/not-a-uuid",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));
}

#[tokio::test]
async fn job_create_rejects_missing_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::Operator,
        "missing-printer-operator",
    )
    .await;

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/00000000-0000-0000-0000-000000000001/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "printer_not_found" }));
}

#[tokio::test]
async fn job_create_rejects_empty_invalid_and_oversized_artifacts() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "invalid-artifact-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "artifact_empty" }));

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body(None, None, 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "artifact_invalid_upload" }));

    let oversized = vec![0_u8; state.job_storage().max_artifact_bytes() + 1];
    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &uri,
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", &oversized)), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body, json!({ "error": "artifact_too_large" }));
}

#[tokio::test]
async fn job_create_defaults_content_type_and_rejects_invalid_plate() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "default-content-type-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body_with_fields(
            Some(("plate.3mf", "", b"abc")),
            &[
                ("filename", "plate.3mf"),
                ("content_type", ""),
                ("plate_id", "1"),
                ("use_ams", "false"),
                ("flow_cali", "false"),
                ("timelapse", "false"),
            ],
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["artifact"]["content_type"], "application/octet-stream");

    for plate_id in [0, -1] {
        let (status, body) = multipart_request_as(
            app.clone(),
            Method::POST,
            &uri,
            multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), plate_id),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "artifact_invalid_plate" }));
    }

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &uri,
        multipart_print_body(
            None,
            Some(("plate.3mf", "model/3mf", b"abc")),
            4294967296_i64,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "artifact_invalid_plate" }));
}
