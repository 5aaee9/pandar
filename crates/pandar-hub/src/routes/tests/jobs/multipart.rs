use super::*;

#[tokio::test]
async fn job_create_rejects_missing_empty_oversized_and_invalid_multipart_uploads() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "multipart-validation-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body(None, None, 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "artifact_invalid_upload" }));

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "artifact_empty" }));

    let oversized = vec![b'x'; state.artifact_storage().max_artifact_bytes() + 1];
    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", &oversized)), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response, json!({ "error": "artifact_too_large" }));
}

#[tokio::test]
async fn job_create_rejects_invalid_and_missing_printer_and_bad_plate() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "printer-validation-job-operator",
    )
    .await;
    let missing_printer_id = uuid::Uuid::new_v4().to_string();

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "invalid_printer_id" }));

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{missing_printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(response, json!({ "error": "printer_not_found" }));

    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (status, response) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 0),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "artifact_invalid_plate" }));
}

#[tokio::test]
async fn job_create_rejects_oversized_text_fields_before_buffering() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "multipart-text-field-limit-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let huge_mapping = "x".repeat(16 * 1024 + 1);

    let (status, response) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body_with_fields(
            Some(("plate.3mf", "model/3mf", b"abc")),
            &[
                ("filename", "plate file.3mf"),
                ("content_type", "model/3mf"),
                ("plate_id", "1"),
                ("use_ams", "true"),
                ("flow_cali", "false"),
                ("timelapse", "true"),
                ("ams_mapping", &huge_mapping),
            ],
        ),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "artifact_invalid_upload" }));
}

#[tokio::test]
async fn job_create_deletes_stored_artifact_when_repository_fails() {
    let temp_dir = tempfile::tempdir().unwrap();
    let artifact_storage = crate::artifacts::FilesystemArtifactStorage::new(
        temp_dir.path(),
        crate::artifacts::DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let state = AppState::connect_with_config_values(
        "sqlite::memory:",
        artifact_storage,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "repo-failure-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_job_artifact_insert BEFORE INSERT ON job_artifacts
         BEGIN
           SELECT RAISE(FAIL, 'forced job_artifacts insert failure');
         END",
    )
    .execute(sqlite_pool(&state))
    .await
    .unwrap();

    let (status, response) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response, json!({ "error": "internal_server_error" }));

    let artifacts: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM job_artifacts")
        .fetch_one(sqlite_pool(&state))
        .await
        .unwrap();
    assert_eq!(artifacts.0, 0);
    assert_eq!(count_files(temp_dir.path()), 0);
}

#[tokio::test]
async fn job_create_removes_staged_upload_when_later_text_field_is_invalid() {
    let existing_temp_uploads = temp_upload_paths();
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "post-file-validation-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();

    let (status, response) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body_file_first(
            ("plate.3mf", "model/3mf", b"abc"),
            &[
                ("filename", "plate.3mf"),
                ("content_type", "model/3mf"),
                ("plate_id", "not-a-number"),
                ("use_ams", "true"),
                ("flow_cali", "false"),
                ("timelapse", "true"),
            ],
        ),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "bad_request" }));
    eventually_no_new_temp_uploads(&existing_temp_uploads).await;
}

fn count_files(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() { count_files(&path) } else { 1 }
        })
        .sum()
}

fn temp_upload_paths() -> std::collections::HashSet<std::path::PathBuf> {
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|entry| {
            entry
                .ok()
                .filter(|entry| {
                    entry
                        .file_name()
                        .into_string()
                        .is_ok_and(|name| name.starts_with("pandar-upload-"))
                })
                .map(|entry| entry.path())
        })
        .collect()
}

async fn eventually_no_new_temp_uploads(existing: &std::collections::HashSet<std::path::PathBuf>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let leaked: Vec<_> = temp_upload_paths().difference(existing).cloned().collect();
        if leaked.is_empty() {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("staged uploads were not cleaned up: {leaked:?}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

fn sqlite_pool(state: &AppState) -> &sqlx::SqlitePool {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    pool
}
