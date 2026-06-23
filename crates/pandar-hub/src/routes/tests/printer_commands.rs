use super::*;
use pandar_core::AgentId;
use tokio::sync::mpsc;

#[tokio::test]
async fn discover_printers_requires_operator_role() {
    let state = state().await;
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
        Some(json!({ "timeout_seconds": 5 })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn discover_printers_rejects_invalid_timeout_payloads() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    for payload in [
        json!({ "timeout_seconds": 0 }),
        json!({ "timeout_seconds": 16 }),
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
        assert_eq!(body, json!({ "error": "invalid_discovery_timeout" }));
    }

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        Some(json!({ "timeout_seconds": "bad" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));

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
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body, json!({ "error": "bad_request" }));
}

#[tokio::test]
async fn discover_printers_defaults_timeout_audits_and_wakes_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
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
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/discover-printers"),
        Some(json!({})),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "discover_printers");
    assert_eq!(body["result_json"], Value::Null);
    assert_eq!(
        body["payload_json"],
        json!({ "timeout_seconds": 5 }).to_string()
    );
    wake_receiver.try_recv().expect("agent should be woken");
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
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

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
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        body["payload_json"],
        json!({ "timeout_seconds": 5 }).to_string()
    );
}

#[tokio::test]
async fn diagnose_printer_rejects_access_code_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    let access_code = "ACCESS-CODE-SHOULD-NOT-LEAK";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        Some(json!({
            "serial_number": "BAMBU123",
            "access_code": access_code
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains(access_code));
}

#[tokio::test]
async fn diagnose_printer_enqueues_redacted_payload_audits_and_wakes_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(tenant["id"].as_str().unwrap()).unwrap();
    let agent_id = AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
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
        })
        .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/diagnose-printer"),
        Some(json!({ "serial_number": "BAMBU123" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "diagnose_printer");
    assert_eq!(
        body["payload_json"],
        json!({ "serial_number": "BAMBU123" }).to_string()
    );
    wake_receiver.try_recv().expect("agent should be woken");
    let events = state
        .audit_events()
        .list_for_tenant(tenant_id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.diagnose_printer")
        .expect("diagnostic audit event");
    let metadata = serde_json::from_str::<serde_json::Value>(&event.metadata_json).unwrap();
    assert!(metadata["tenant_token_id"].as_str().is_some());
    assert_eq!(metadata["tenant_token_scopes"], json!(["*"]));
}

#[tokio::test]
async fn command_detail_requires_viewer_and_returns_result_json() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let agent = state
        .agents()
        .create(tenant.id, "shop-agent")
        .await
        .unwrap();
    let viewer_token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-command-detail",
    )
    .await;
    let command = state
        .commands()
        .enqueue_discover_printers(
            tenant.id,
            agent.id,
            crate::repositories::DiscoverPrintersPayload { timeout_seconds: 5 },
        )
        .await
        .unwrap();
    let result_json = json!({
        "type": "printer_discovery",
        "printers": [{
            "serial_number": "BAMBU123",
            "host": "192.0.2.10",
            "name": "Shop A1",
            "model": "A1",
            "source": "ssdp"
        }]
    })
    .to_string();
    state
        .commands()
        .mark_sent(command.id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .commands()
        .mark_succeeded_with_result(command.id, tenant.id, agent.id, Some(result_json.clone()))
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/commands/{}", tenant.id, command.id),
        None,
        &viewer_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], command.id.to_string());
    assert_eq!(body["result_json"], result_json);

    let other_tenant = state.tenants().create("other", "Other Labs").await.unwrap();
    let other_token = auth_token_for_role(
        &state,
        &other_tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "other-command-detail",
    )
    .await;
    let (status, body) = request_as(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/commands/{}",
            other_tenant.id, command.id
        ),
        None,
        &other_token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "command_not_found" }));
}

#[tokio::test]
async fn invalid_command_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/commands/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_command_id" }));
}

#[tokio::test]
async fn invalid_agent_id_on_refresh_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/not-a-uuid/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_agent_id" }));
}
