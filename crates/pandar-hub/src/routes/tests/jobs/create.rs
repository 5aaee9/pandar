use super::*;

#[derive(Debug, serde::Deserialize)]
struct SlicerMetadata {
    display_name: String,
    default_plate_id: i64,
    #[serde(flatten)]
    _extra: std::collections::BTreeMap<String, serde_json::Value>,
}

struct InvalidMaterialMappingCase {
    ams_mapping: Option<serde_json::Value>,
    ams_mapping2: Option<serde_json::Value>,
}

#[tokio::test]
async fn job_create_writes_artifact_queues_command_and_returns_created_job() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate file.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<JobResponse>(body);
    assert_eq!(body.status, "queued");
    assert_eq!(body.print.status, "pending");
    assert_eq!(body.print.progress_percent, None);
    assert_eq!(body.printer_id, printer_id.to_string());
    assert_eq!(body.command.kind, "print_project_file");
    assert_eq!(body.command.status, "queued");
    assert_eq!(body.artifact.filename, "plate_file.3mf");
    assert_eq!(body.artifact.size_bytes, 3);
    assert_eq!(body.material.ams_mapping, None);
    assert_eq!(body.material.ams_mapping2, None);
    assert!(body.material.filament_usage.is_empty());
    assert_eq!(state.commands().count().await.unwrap(), 1);
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    assert!(events.iter().any(|event| event.action == "job.create"));
}

#[tokio::test]
async fn job_create_persists_parsed_artifact_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "metadata-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("project.3mf", "model/3mf", &artifact)), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<JobResponse>(body);
    let metadata = decode::<SlicerMetadata>(body.artifact.metadata.clone().unwrap());
    assert_eq!(metadata.display_name, "plate file");
    assert_eq!(metadata.default_plate_id, 1);

    let (status, list) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = decode::<JobListResponse>(list);
    assert_eq!(list.jobs[0].artifact.metadata, body.artifact.metadata);
}

#[tokio::test]
async fn print_job_wakes_agent_on_sibling_instance() {
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
        "sibling-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
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

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        decode::<JobResponse>(body).command.kind,
        "print_project_file"
    );
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn print_job_returns_created_when_agent_wake_publish_fails() {
    let state = state()
        .await
        .with_control_plane_for_tests(crate::cluster::ControlPlane::failing_for_tests());
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "failed-wake-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs"),
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        decode::<JobResponse>(body).command.kind,
        "print_project_file"
    );
    assert_eq!(state.commands().count().await.unwrap(), 1);
    assert!(
        state
            .metrics()
            .control_plane_snapshot()
            .contains(&("publish_failed", 1))
    );
}

#[tokio::test]
async fn job_create_accepts_optional_material_mappings_and_responses_preserve_null_vs_empty() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "mapping-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    let (status, null_mapping) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body(None, Some(("plate.3mf", "model/3mf", b"abc")), 1),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let null_mapping = decode::<JobResponse>(null_mapping);
    assert_eq!(null_mapping.material.ams_mapping, None);
    assert_eq!(null_mapping.material.ams_mapping2, None);

    let (status, empty_mapping) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body_with_mappings(
            None,
            Some(("plate.3mf", "model/3mf", b"abc")),
            1,
            Some(json!([])),
            Some(json!([])),
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let empty_mapping = decode::<JobResponse>(empty_mapping);
    assert_eq!(empty_mapping.material.ams_mapping, Some(vec![]));
    assert_eq!(empty_mapping.material.ams_mapping2, Some(vec![]));

    let (status, external_mapping) = multipart_request_as(
        app.clone(),
        Method::POST,
        &uri,
        multipart_print_body_with_mappings(
            None,
            Some(("external.3mf", "model/3mf", b"abc")),
            1,
            None,
            Some(json!([{ "ams_id": 254, "slot_id": 8 }])),
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let external_mapping = decode::<JobResponse>(external_mapping);
    assert_eq!(
        external_mapping.material.ams_mapping2,
        Some(vec![AmsMapping2Entry {
            ams_id: 254,
            slot_id: 8
        }])
    );

    let (status, list) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = decode::<JobListResponse>(list);
    assert!(list.jobs.iter().any(|job| {
        job.id == null_mapping.id
            && job.material.ams_mapping.is_none()
            && job.material.ams_mapping2.is_none()
    }));
    assert!(list.jobs.iter().any(|job| {
        job.id == empty_mapping.id
            && job.material.ams_mapping == Some(vec![])
            && job.material.ams_mapping2 == Some(vec![])
    }));
    assert!(list.jobs.iter().any(|job| {
        job.id == external_mapping.id
            && job.material.ams_mapping2
                == Some(vec![AmsMapping2Entry {
                    ams_id: 254,
                    slot_id: 8,
                }])
    }));

    let (status, detail) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/jobs/{}", empty_mapping.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let detail = decode::<JobResponse>(detail);
    assert_eq!(detail.material.ams_mapping, Some(vec![]));
    assert_eq!(detail.material.ams_mapping2, Some(vec![]));
}

#[tokio::test]
async fn job_create_rejects_invalid_material_mapping_shapes_without_echoing_values() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Operator,
        "invalid-mapping-job-operator",
    )
    .await;
    let agent = state.agents().create(tenant_id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent.id)
        .await
        .unwrap();
    let uri = format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs");

    for payload in [
        InvalidMaterialMappingCase {
            ams_mapping: Some(json!("sk-live-secret")),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: Some(json!(["sk-live-secret"])),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: Some(json!([2147483648_i64])),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: Some(json!(vec![0; 33])),
            ams_mapping2: None,
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(json!("sk-live-secret")),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(json!([{ "ams_id": "sk-live-secret", "slot_id": 0 }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(json!([{ "ams_id": 0, "slot_id": 2147483648_i64 }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(
                json!([{ "ams_id": 0, "slot_id": 0, "password": "sk-live-secret" }]),
            ),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(json!([{ "ams_id": 0, "slot_id": 0, "token": "sk-live-secret" }])),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(
                json!([{ "ams_id": 0, "slot_id": 0, "access_code": "sk-live-secret" }]),
            ),
        },
        InvalidMaterialMappingCase {
            ams_mapping: None,
            ams_mapping2: Some(json!(vec![json!({ "ams_id": 0, "slot_id": 0 }); 33])),
        },
    ] {
        let (status, response) = multipart_request_as(
            app.clone(),
            Method::POST,
            &uri,
            multipart_print_body_with_mappings(
                None,
                Some(("plate.3mf", "model/3mf", b"abc")),
                1,
                payload.ams_mapping,
                payload.ams_mapping2,
            ),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let response_text = response.to_string();
        assert_eq!(
            decode::<ErrorResponse>(response).error,
            "invalid_material_mapping"
        );
        assert!(!response_text.contains("sk-live-secret"));
    }
}
