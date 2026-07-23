use super::*;

#[tokio::test]
async fn plugin_print_returns_job_shape_and_records_plugin_actor_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print", "Plugin Print")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "print-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
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

    assert_eq!(status, StatusCode::CREATED);
    assert!(body.get("command_id").is_none());
    assert!(body.get("pandar_job_id").is_none());
    let body = decode::<PluginPrintResponse>(body);
    assert_eq!(body.status, "queued");
    assert!(body.task_id > 0);
    assert_eq!(body.task_id, body.studio_submission_id);

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.create")
        .unwrap();
    let metadata: PluginTokenAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(event.actor_type, "plugin_token");
    assert!(!metadata.tenant_token_id.is_empty());
    assert_eq!(
        metadata.tenant_token_scopes,
        vec!["plugin:studio".to_owned()]
    );
    assert_eq!(metadata.token, None);
    assert_eq!(metadata.ticket, None);
}

#[tokio::test]
async fn plugin_print_handles_concurrent_sqlite_writes() {
    let state = AppState::file_sqlite_for_tests()
        .await
        .unwrap()
        .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-concurrent", "Plugin Print Concurrent")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let mut requests = Vec::new();
    for index in 0..2 {
        let token = plugin_studio_tenant_token(
            &state,
            &tenant.id.to_string(),
            &format!("print-plugin-{index}"),
        )
        .await;
        let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
            .await
            .unwrap();
        requests.push((token, printer_id));
    }

    let responses =
        futures_util::future::join_all(requests.into_iter().map(|(token, printer_id)| {
            let app = app.clone();
            async move {
                multipart_request_as(
                    app,
                    Method::POST,
                    "/api/v1/plugin/prints",
                    multipart_print_body(
                        Some(&printer_id),
                        Some(("plugin concurrent.3mf", "model/3mf", b"abc")),
                        1,
                    ),
                    &token,
                )
                .await
            }
        }))
        .await;

    for (status, body) in responses {
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let body = decode::<PluginPrintResponse>(body);
        assert_eq!(body.status, "queued");
        assert!(body.task_id > 0);
        assert_eq!(body.task_id, body.studio_submission_id);
    }
}

#[tokio::test]
async fn plugin_print_and_list_use_stable_studio_task_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-metadata", "Plugin Print Metadata")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "print-metadata-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let artifact = crate::routes::tests::multipart::slicer_metadata_fixture();

    let (status, body) = multipart_request_as(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", &artifact)),
            1,
        ),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let body = decode::<PluginPrintResponse>(body);

    let (status, list) = request_as(app, Method::GET, "/api/v1/plugin/jobs", None, &token).await;
    assert_eq!(status, StatusCode::OK);
    let list = decode::<PluginJobListResponse>(list);
    assert_eq!(list.total, 1);
    assert_eq!(list.hits[0].id, body.studio_submission_id);
    assert_eq!(list.hits[0].title, "plate file.3mf");
}

#[tokio::test]
async fn plugin_print_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-sibling", "Plugin Print Sibling")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "sibling-print-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _) = tokio::sync::mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let (status, body) = multipart_request_as(
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

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(decode::<PluginPrintResponse>(body).status, "queued");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}
