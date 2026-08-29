use super::*;

#[tokio::test]
async fn printer_control_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant.id,
        agent.id,
        Some("A1"),
    )
    .await
    .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-control-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/printers/{printer_id}/controls",
            tenant.id
        ),
        printer_control_body(PrinterControlRequest::action("pause")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn printer_control_enqueues_audits_and_wakes_owning_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    let (wake_sender, mut wake_receiver) = mpsc::channel(1);
    let (close_sender, _) = mpsc::channel(1);
    state
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id,
            name: "shop-agent".to_owned(),
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

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_control_body(PrinterControlRequest::set_print_speed(4)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "printer_operation");
    assert_eq!(body.agent_id, agent_id.to_string());
    assert_eq!(body.printer_id.as_deref(), Some(printer_id.as_str()));
    let payload: PrinterOperationPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.printer_id, printer_id);
    assert_eq!(payload.serial_number, format!("serial-{printer_id}"));
    assert_eq!(payload.operation.kind, "set_print_speed");
    assert_eq!(payload.operation.speed_mode, Some(4));
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("owning agent should be woken")
        .expect("wake channel should stay open");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "printer.dispatch_control")
        .expect("printer control audit event");
    assert_eq!(event.target_type, "printer");
    assert_eq!(event.target_id.as_deref(), Some(printer_id.as_str()));
    let metadata: PrinterControlAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(metadata.agent_id, agent_id.to_string());
    assert_eq!(metadata.serial_number, format!("serial-{printer_id}"));
    assert_eq!(metadata.action, "set_print_speed");
    assert_eq!(metadata.speed_mode, 4);
}

#[tokio::test]
async fn printer_control_rejects_unknown_model_before_command_or_audit_insert() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("Mystery Model"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_control_body(PrinterControlRequest::action("pause")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "printer_control_unavailable"
    );
    assert_eq!(state.commands().count().await.unwrap(), 0);
    assert_no_printer_control_audit(&state, tenant_id).await;
}

#[tokio::test]
async fn printer_control_wakes_owning_agent_not_sibling() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let owner_agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let other_agent = state
        .agents()
        .create(tenant_id, "other-agent")
        .await
        .unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        owner_agent_id,
        Some("A1"),
    )
    .await
    .unwrap();
    let (owner_wake_sender, mut owner_wake_receiver) = mpsc::channel(1);
    let (other_wake_sender, mut other_wake_receiver) = mpsc::channel(1);
    let (owner_close_sender, _) = mpsc::channel(1);
    let (other_close_sender, _) = mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: owner_agent_id,
            name: "shop-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: owner_wake_sender,
            close_sender: owner_close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id,
            agent_id: other_agent.id,
            name: "other-agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender: other_wake_sender,
            close_sender: other_close_sender,
            command_sender: tokio::sync::mpsc::channel(1).0,
            capabilities: std::collections::HashSet::new(),
            pending_live_commands: crate::sessions::empty_pending_live_commands(),
            live_command_transition: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        printer_control_body(PrinterControlRequest::action("resume")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode::<CommandResponse>(body).agent_id,
        owner_agent_id.to_string()
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        owner_wake_receiver.recv(),
    )
    .await
    .expect("owning agent should be woken")
    .expect("wake channel should stay open");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            other_wake_receiver.recv()
        )
        .await
        .is_err()
    );
}
