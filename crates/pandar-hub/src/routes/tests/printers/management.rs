use super::*;

#[tokio::test]
async fn printer_camera_stream_opens_agent_camera_tunnel() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await
        .unwrap()
        .unwrap();
    let (wake_sender, _wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _close_receiver) = tokio::sync::mpsc::channel(1);
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "garage".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
            command_sender,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let response = raw_request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/camera.mp4"),
        &token,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "video/mp4"
    );
    let command = command_receiver.recv().await.unwrap().unwrap();
    match command.command.unwrap() {
        hub_command::Command::CameraStream(command) => match command.command.unwrap() {
            hub_camera_command::Command::Open(open) => {
                assert_eq!(open.serial_number, printer.serial_number);
                assert_eq!(open.mode, CameraStreamMode::FragmentedMp4 as i32);
            }
            other => panic!("expected open camera stream command, got {other:?}"),
        },
        other => panic!("expected camera stream command, got {other:?}"),
    }
}

#[tokio::test]
async fn tenant_admin_can_delete_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<PrinterResponse>(body).id, printer_id);

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode::<PrinterListResponse>(body).printers.is_empty());

    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.delete")
        .expect("printer delete audit event");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
}

#[tokio::test]
async fn viewer_cannot_delete_printer() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-delete-printer",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn missing_printer_detail_returns_not_found() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let printer_id = uuid::Uuid::new_v4();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "printer_not_found");
}

#[tokio::test]
async fn invalid_printer_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_printer_id");
}

#[tokio::test]
async fn refresh_printers_returns_command_record() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.tenant_id, tenant_id);
    assert_eq!(body.agent_id, agent_id);
    assert_eq!(body.kind, "refresh_printers");
    assert_eq!(body.status, "queued");
    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(&tenant_id).unwrap())
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "agent.refresh_printers")
    );
}

#[tokio::test]
async fn refresh_printer_materials_enqueues_for_owning_agent_and_wakes_it() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant_id, agent_id)
        .await
        .unwrap();
    let mut wake_receiver =
        register_route_test_session_with_wake(&state, tenant_id, agent_id).await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/materials:refresh"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "refresh_printer_materials");
    assert_eq!(body.agent_id, agent_id.to_string());
    assert_eq!(body.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: RefreshPrinterMaterialsPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(payload.serial_number, format!("serial-{printer_id}"));
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");

    let audit = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    assert!(
        audit
            .iter()
            .any(|event| event.action == "printer.refresh_materials")
    );
}

#[tokio::test]
async fn refresh_printer_materials_rejects_invalid_and_missing_printers() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/not-a-uuid/materials:refresh"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_printer_id");

    let missing = uuid::Uuid::new_v4();
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{missing}/materials:refresh"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "printer_not_found");
}
