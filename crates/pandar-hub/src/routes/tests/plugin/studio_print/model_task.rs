use super::*;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StudioModelTaskResponse {
    job_id: i32,
    design_id: i32,
    profile_id: i32,
    instance_id: i32,
    task_id: String,
    model_id: String,
    model_name: String,
    profile_name: String,
}

#[tokio::test]
async fn ordinary_studio_model_task_returns_only_real_persisted_metadata() {
    let (_, app, _, _, printer_id, token) = studio_fixture("studio-model-task-ordinary").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!(
            "/api/v1/plugin/jobs/{}/model-task",
            created.studio_submission_id
        ),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        decode::<StudioModelTaskResponse>(body),
        StudioModelTaskResponse {
            job_id: created.studio_submission_id,
            design_id: 0,
            profile_id: 0,
            instance_id: 0,
            task_id: created.studio_submission_id.to_string(),
            model_id: String::new(),
            model_name: "Fixture project".to_owned(),
            profile_name: "Fixture preset".to_owned(),
        }
    );
}

#[tokio::test]
async fn studio_model_task_requires_the_canonical_decimal_submission_id() {
    let (_, app, _, _, printer_id, token) = studio_fixture("studio-model-task-canonical-id").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;

    for id in [
        format!("0{}", created.studio_submission_id),
        format!("+{}", created.studio_submission_id),
        "0".to_owned(),
        "-1".to_owned(),
        (i64::from(i32::MAX) + 1).to_string(),
        "not-a-number".to_owned(),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!("/api/v1/plugin/jobs/{id}/model-task"),
            None,
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{id}: {body}");
        assert_eq!(
            body["error"],
            serde_json::json!("invalid_studio_submission_id"),
            "{id}"
        );
    }
}

#[tokio::test]
async fn studio_model_task_uses_only_the_authenticated_tenant() {
    let (state, app, _, _, printer_id, token) = studio_fixture("studio-model-task-tenant-a").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let other_tenant = state
        .tenants()
        .create("studio-model-task-tenant-b", "Studio Model Task Tenant B")
        .await
        .unwrap();
    let other_token = plugin_studio_tenant_token(
        &state,
        &other_tenant.id.to_string(),
        "studio-model-task-tenant-b",
    )
    .await;

    for (case, id) in [
        ("cross tenant", created.studio_submission_id),
        ("unknown", created.studio_submission_id + 1),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!("/api/v1/plugin/jobs/{id}/model-task"),
            None,
            &other_token,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{case}: {body}");
        assert_eq!(body["error"], serde_json::json!("job_not_found"), "{case}");
    }
}

#[tokio::test]
async fn studio_model_task_rejects_missing_invalid_or_makerworld_metadata() {
    let (state, app, tenant_id, _, printer_id, token) =
        studio_fixture("studio-model-task-invalid-metadata").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let id =
        pandar_core::StudioSubmissionId::try_from(i64::from(created.studio_submission_id)).unwrap();
    let job = state
        .jobs()
        .get_by_studio_submission_id(tenant_id, id)
        .await
        .unwrap()
        .unwrap();
    let base = serde_json::to_value(job.job.studio_metadata.unwrap()).unwrap();
    let cases = [
        ("missing", None),
        ("corrupt", Some("{".to_owned())),
        (
            "empty project name",
            Some(metadata_with(&base, "project_name", serde_json::json!(""))),
        ),
        (
            "empty preset name",
            Some(metadata_with(&base, "preset_name", serde_json::json!("  "))),
        ),
        (
            "negative design id",
            Some(metadata_with(&base, "stl_design_id", serde_json::json!(-1))),
        ),
        (
            "out of range profile id",
            Some(metadata_with(
                &base,
                "origin_profile_id",
                serde_json::json!(i64::from(i32::MAX) + 1),
            )),
        ),
        (
            "MakerWorld design id",
            Some(metadata_with(&base, "stl_design_id", serde_json::json!(42))),
        ),
        (
            "MakerWorld profile id",
            Some(metadata_with(
                &base,
                "origin_profile_id",
                serde_json::json!(43),
            )),
        ),
        (
            "MakerWorld model id",
            Some(metadata_with(
                &base,
                "origin_model_id",
                serde_json::json!("model-44"),
            )),
        ),
    ];
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };

    for (case, metadata_json) in cases {
        sqlx::query(
            "UPDATE jobs SET studio_metadata_json = ?1 WHERE tenant_id = ?2 AND studio_submission_id = ?3",
        )
        .bind(metadata_json)
        .bind(tenant_id.to_string())
        .bind(created.studio_submission_id)
        .execute(pool)
        .await
        .unwrap();
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!(
                "/api/v1/plugin/jobs/{}/model-task",
                created.studio_submission_id
            ),
            None,
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT, "{case}: {body}");
        assert_eq!(
            body["error"],
            serde_json::json!("studio_model_task_metadata_unavailable"),
            "{case}"
        );
    }
}

fn metadata_with(base: &serde_json::Value, field: &str, value: serde_json::Value) -> String {
    let mut metadata = base.clone();
    metadata[field] = value;
    serde_json::to_string(&metadata).unwrap()
}

#[tokio::test]
async fn studio_model_task_requires_a_current_plugin_session() {
    let (_, app, _, _, printer_id, token) = studio_fixture("studio-model-task-session").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let uri = format!(
        "/api/v1/plugin/jobs/{}/model-task",
        created.studio_submission_id
    );

    let (status, body) = request(app.clone(), Method::GET, &uri, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"], serde_json::json!("missing_auth_token"));

    let revoked = raw_request_as(
        app.clone(),
        Method::DELETE,
        "/api/v1/plugin/session",
        &token,
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let (status, body) = request_as(app, Method::GET, &uri, None, &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"], serde_json::json!("invalid_auth_token"));
}
