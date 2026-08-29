use super::*;

#[tokio::test]
async fn discover_printers_requires_operator_role() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-discover-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!(
            "/api/v1/tenants/{}/agents/{}/discover-printers",
            tenant.id, agent.id
        ),
        discover_printers_body(5),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn discover_printers_rejects_invalid_timeout_payloads() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    for payload in [
        discover_printers_body(0).unwrap(),
        discover_printers_body(16).unwrap(),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
            Some(payload),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            decode::<ErrorResponse>(body).error,
            "invalid_discovery_timeout"
        );
    }

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        discover_printers_timeout_string_body("bad"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"
                ))
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: ErrorResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(body.error, "bad_request");
}

#[tokio::test]
async fn discover_printers_defaults_timeout_audits_and_wakes_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
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
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        empty_body(),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "discover_printers");
    assert_eq!(body.result_json, None);
    let payload: DiscoverPrintersPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.timeout_seconds, 5);
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.action == "agent.discover_printers")
    );
}

#[tokio::test]
async fn discover_printers_defaults_empty_json_body() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"
                ))
                .header("content-type", "application/json")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: CommandResponse = serde_json::from_slice(&body).unwrap();
    let payload: DiscoverPrintersPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.timeout_seconds, 5);
}

#[tokio::test]
async fn diagnose_printer_rejects_access_code_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let access_code = "ACCESS-CODE-SHOULD-NOT-LEAK";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        diagnose_printer_with_access_code_body("BAMBU123", access_code),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains(access_code));
}

#[tokio::test]
async fn diagnose_printer_enqueues_redacted_payload_audits_and_wakes_agent() {
    let state = state().await;
    let _control_plane = start_control_plane(state.clone()).await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
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
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        diagnose_printer_body("BAMBU123"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "diagnose_printer");
    let payload: DiagnosePrinterPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.serial_number, "BAMBU123");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("agent should be woken")
        .expect("wake channel should stay open");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.diagnose_printer")
        .expect("diagnostic audit event");
    let metadata: TenantTokenAuditMetadata = serde_json::from_str(&event.metadata_json).unwrap();
    assert!(!metadata.tenant_token_id.is_empty());
    assert_eq!(metadata.tenant_token_scopes, vec!["*".to_string()]);
}
