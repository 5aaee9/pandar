use super::*;

#[tokio::test]
async fn plugin_print_uses_stable_artifact_validation_errors() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-validation", "Plugin Print Validation")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "print-validation-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(Some(&printer_id), None, 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "artifact_invalid_upload" }));

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", b"")),
            1,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "artifact_empty" }));

    let oversized = vec![b'x'; state.artifact_storage().max_artifact_bytes() + 1];
    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", &oversized)),
            1,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response, json!({ "error": "artifact_too_large" }));

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some("not-a-uuid"),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response, json!({ "error": "invalid_printer_id" }));

    let (status, response) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&uuid::Uuid::new_v4().to_string()),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(response, json!({ "error": "printer_not_found" }));

    for plate_id in [0, -1, 4294967296_i64] {
        let (status, response) = multipart_request_as(
            app.clone(),
            Method::POST,
            "/api/v1/plugin/prints",
            multipart_print_body(
                Some(&printer_id),
                Some(("plugin plate.3mf", "model/3mf", b"abc")),
                plate_id,
            ),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response, json!({ "error": "artifact_invalid_plate" }));
    }
}

#[tokio::test]
async fn plugin_print_deletes_stored_artifact_when_repository_fails() {
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
    let tenant = state
        .tenants()
        .create("plugin-print-repo-failure", "Plugin Print Repo Failure")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "repo-failure-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_plugin_job_artifact_insert BEFORE INSERT ON job_artifacts
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
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
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

fn count_files(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() { count_files(&path) } else { 1 }
        })
        .sum()
}

fn sqlite_pool(state: &AppState) -> &sqlx::SqlitePool {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    pool
}
