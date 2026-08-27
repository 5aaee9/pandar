use super::*;

mod failure;
mod model_task;

#[derive(Debug, Deserialize)]
struct CreatedPrint {
    task_id: i32,
    studio_submission_id: i32,
    status: String,
}

async fn studio_fixture(
    slug: &str,
) -> (
    AppState,
    Router,
    TenantId,
    pandar_core::AgentId,
    String,
    String,
) {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create(slug, slug).await.unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), slug).await;
    (state, app, tenant.id, agent.id, printer_id, token)
}

async fn create_print(app: Router, printer_id: &str, token: &str, artifact: &[u8]) -> CreatedPrint {
    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(printer_id),
            Some(("plate file.3mf", "model/3mf", artifact)),
            1,
        ),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created = decode::<CreatedPrint>(body);
    assert_eq!(created.task_id, created.studio_submission_id);
    assert_eq!(created.status, "queued");
    created
}

#[tokio::test]
async fn studio_task_routes_use_stable_numeric_ids_and_tenant_server_filters() {
    let (state, app, tenant_id, _, printer_id, token) = studio_fixture("studio-task-routes").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let first = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let second = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();

    let uri = format!(
        "/api/v1/plugin/jobs?dev_id={}&status=1&offset=0&limit=1",
        printer.serial_number
    );
    let (status, page) = request_as(app.clone(), Method::GET, &uri, None, &token).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], serde_json::json!(2));
    assert_eq!(page["hits"].as_array().unwrap().len(), 1);
    let hit = &page["hits"][0];
    assert_eq!(hit["id"], serde_json::json!(second.studio_submission_id));
    assert_eq!(hit["status"], serde_json::json!(1));
    assert_eq!(hit["designId"], serde_json::json!(0));
    assert_eq!(
        hit["profileId"],
        serde_json::json!(second.studio_submission_id)
    );
    assert_eq!(hit["deviceId"], serde_json::json!(printer.serial_number));
    assert_eq!(hit["deviceName"], serde_json::json!(printer.name));

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/plugin/jobs/{}", first.studio_submission_id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["studio_submission_id"],
        serde_json::json!(first.task_id)
    );
    assert_eq!(body["job_status"], serde_json::json!("queued"));
    assert_eq!(body["print_status"], serde_json::json!("pending"));

    let other = state
        .tenants()
        .create("studio-task-other", "Studio Task Other")
        .await
        .unwrap();
    let other_token =
        plugin_studio_tenant_token(&state, &other.id.to_string(), "studio-task-other").await;
    let (status, _) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/plugin/jobs/{}", first.studio_submission_id),
        None,
        &other_token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn studio_task_list_rejects_unknown_and_cross_tenant_device_filters() {
    let (state, app, _, _, _, token) = studio_fixture("studio-task-device-filter").await;
    let other_tenant = state
        .tenants()
        .create(
            "studio-task-device-filter-other",
            "Studio Task Device Filter Other",
        )
        .await
        .unwrap();
    let other_agent = state
        .agents()
        .create(other_tenant.id, "agent")
        .await
        .unwrap();
    let other_printer_id =
        insert_printer_fixture(state.database(), other_tenant.id, other_agent.id)
            .await
            .unwrap();
    let other_printer = state
        .printers()
        .get_for_tenant(other_tenant.id, &other_printer_id)
        .await
        .unwrap()
        .unwrap();

    for serial in [
        "UNKNOWN-STUDIO-DEVICE",
        other_printer.serial_number.as_str(),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!("/api/v1/plugin/jobs?dev_id={serial}"),
            None,
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["error"], serde_json::json!("printer_not_found"));
    }
}

#[tokio::test]
async fn studio_plate_and_subtask_are_typed_and_artifact_backed() {
    let (_, app, _, _, printer_id, token) = studio_fixture("studio-subtask").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;

    let (status, plate) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/plugin/jobs/{}/plate", created.studio_submission_id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plate}");
    assert_eq!(plate["plate_index"], serde_json::json!(1));

    let (status, subtask) = request_as(
        app,
        Method::GET,
        &format!(
            "/api/v1/plugin/jobs/{}/subtask",
            created.studio_submission_id
        ),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{subtask}");
    let content: serde_json::Value =
        serde_json::from_str(subtask["content"].as_str().unwrap()).unwrap();
    assert_eq!(content["info"]["plate_idx"], serde_json::json!(1));
    let plate = &subtask["context"]["plates"][0];
    assert_eq!(plate["index"], serde_json::json!(1));
    assert!(plate["prediction"].is_i64());
    assert!(plate["weight"].is_number());
    assert!(plate["filaments"][0]["used_g"].is_string());
    assert!(plate["filaments"][0]["used_m"].is_string());
}

#[tokio::test]
async fn studio_subtask_rejects_unusable_persisted_metadata_with_conflict() {
    let (state, app, tenant_id, _, printer_id, token) =
        studio_fixture("studio-subtask-invalid").await;
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();
    let created = create_print(app.clone(), &printer_id, &token, &artifact).await;
    let task = state
        .jobs()
        .get_by_studio_submission_id(
            tenant_id,
            pandar_core::StudioSubmissionId::try_from(i64::from(created.studio_submission_id))
                .unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    let metadata: crate::artifacts::metadata::ArtifactMetadata =
        serde_json::from_str(task.artifact.metadata_json.as_deref().unwrap()).unwrap();

    let mut empty_color = metadata.clone();
    empty_color.plates[0].filaments[0].color = Some(String::new());
    let mut empty_type = metadata.clone();
    empty_type.plates[0].filaments[0].filament_type = Some("  ".to_owned());
    let mut negative_weight = metadata.clone();
    negative_weight.plates[0].filament_weight_grams = Some(-1.0);
    let mut oversized_weight = metadata.clone();
    oversized_weight.plates[0].filament_weight_grams = Some(f64::MAX);
    let mut oversized_prediction = metadata.clone();
    oversized_prediction.plates[0].estimated_time_seconds = Some(i32::MAX as u32 + 1);
    let mut negative_usage = metadata.clone();
    negative_usage.plates[0].filaments[0].used_grams = Some(-1.0);
    let mut oversized_usage = metadata;
    oversized_usage.plates[0].filaments[0].used_meters = Some(f64::MAX);

    let mut cases = vec![
        ("empty color", serde_json::to_string(&empty_color).unwrap()),
        ("empty type", serde_json::to_string(&empty_type).unwrap()),
        (
            "negative weight",
            serde_json::to_string(&negative_weight).unwrap(),
        ),
        (
            "oversized weight",
            serde_json::to_string(&oversized_weight).unwrap(),
        ),
        (
            "oversized prediction",
            serde_json::to_string(&oversized_prediction).unwrap(),
        ),
        (
            "negative usage",
            serde_json::to_string(&negative_usage).unwrap(),
        ),
        (
            "oversized usage",
            serde_json::to_string(&oversized_usage).unwrap(),
        ),
    ];
    cases.push(("malformed JSON", "{".to_owned()));
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    for (case, metadata_json) in cases {
        sqlx::query("UPDATE job_artifacts SET metadata_json = ?1 WHERE id = ?2")
            .bind(metadata_json)
            .bind(&task.artifact.id)
            .execute(pool)
            .await
            .unwrap();
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            &format!(
                "/api/v1/plugin/jobs/{}/subtask",
                created.studio_submission_id
            ),
            None,
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{case}: {body}");
        assert_eq!(
            body["error"],
            serde_json::json!("studio_task_metadata_unavailable"),
            "{case}"
        );
    }
}

#[tokio::test]
async fn studio_cancel_is_confirmed_and_too_late_is_not_faked() {
    let (state, app, tenant_id, agent_id, printer_id, token) =
        studio_fixture("studio-cancel-route").await;
    let first = create_print(app.clone(), &printer_id, &token, b"first").await;

    let cancel_uri = format!("/api/v1/plugin/jobs/{}/cancel", first.studio_submission_id);
    let (status, cancelled) =
        request_as(app.clone(), Method::POST, &cancel_uri, None, &token).await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["job_status"], serde_json::json!("cancelled"));
    assert_eq!(cancelled["print_status"], serde_json::json!("cancelled"));
    let (status, again) = request_as(app.clone(), Method::POST, &cancel_uri, None, &token).await;
    assert_eq!(status, StatusCode::OK, "{again}");

    let second = create_print(app.clone(), &printer_id, &token, b"second").await;
    let job = state
        .jobs()
        .list_for_tenant(tenant_id)
        .await
        .unwrap()
        .into_iter()
        .find(|job| job.job.studio_submission_id.get() == second.studio_submission_id)
        .unwrap();
    state
        .jobs()
        .mark_print_sent(job.job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_acknowledged(job.job.command_id, tenant_id, agent_id)
        .await
        .unwrap();
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/plugin/jobs/{}/cancel", second.studio_submission_id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"], serde_json::json!("cancel_too_late"));
}
