use super::*;

#[tokio::test]
async fn job_list_and_detail_return_tenant_jobs() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "list-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        Some(valid_request()),
        &token,
    )
    .await;
    let job_id = created["id"].as_str().unwrap();

    let (status, list) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["jobs"].as_array().unwrap().len(), 1);

    let (status, detail) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{job_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], created["id"]);
    assert_eq!(detail["command"], created["command"]);
    assert_eq!(detail["print"], created["print"]);
    assert_eq!(detail["material"], created["material"]);
}

#[tokio::test]
async fn job_detail_returns_internal_error_for_corrupt_persisted_mapping_json() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "corrupt-mapping-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        Some(valid_request()),
        &token,
    )
    .await;
    let job_id = created["id"].as_str().unwrap();
    let Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    sqlx::query("UPDATE jobs SET ams_mapping_json = ?2 WHERE id = ?1")
        .bind(job_id)
        .bind(r#"["sk-live-secret"]"#)
        .execute(pool)
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{job_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({ "error": "internal_server_error" }));
    assert!(!body.to_string().contains("sk-live-secret"));
}

#[tokio::test]
async fn missing_job_detail_returns_not_found() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::Viewer,
        "missing-job-viewer",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs/00000000-0000-0000-0000-000000000001"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "job_not_found" }));
}
