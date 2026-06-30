use super::*;

#[tokio::test]
async fn missing_token_on_agent_list_returns_unauthorized() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "missing_auth_token" }));
}

#[tokio::test]
async fn linked_external_jwt_can_read_tenant_resource() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "linked-viewer-read",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [] }));
}

#[tokio::test]
async fn external_jwt_with_unknown_kid_returns_unauthorized() {
    let token = jwt_for(
        "unknown-kid-user",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "unknown-key",
        300,
    );

    let (status, body) = external_jwt_agent_list_response(&token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}

#[tokio::test]
async fn external_jwt_with_wrong_issuer_returns_unauthorized() {
    let token = jwt_for(
        "wrong-issuer-user",
        "https://other-issuer.example.test",
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = external_jwt_agent_list_response(&token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}

#[tokio::test]
async fn external_jwt_with_wrong_audience_returns_unauthorized() {
    let token = jwt_for(
        "wrong-audience-user",
        TEST_ISSUER,
        "api://other-audience",
        "test-key",
        300,
    );

    let (status, body) = external_jwt_agent_list_response(&token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}

#[tokio::test]
async fn expired_external_jwt_returns_unauthorized() {
    let token = jwt_for("expired-user", TEST_ISSUER, TEST_AUDIENCE, "test-key", -120);

    let (status, body) = external_jwt_agent_list_response(&token).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}

#[tokio::test]
async fn valid_unlinked_external_jwt_returns_tenant_forbidden() {
    let token = jwt_for(
        "unlinked-viewer",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = external_jwt_agent_list_response(&token).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));
}

#[tokio::test]
async fn viewer_cannot_create_agent() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "viewer-agent-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn linked_viewer_jwt_cannot_create_agent() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "linked-viewer-create-agent",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn invalid_tenant_id_on_agent_create_returns_bad_request() {
    let (status, body) = request(
        app().await,
        Method::POST,
        "/api/v1/tenants/not-a-uuid/agents",
        Some(json!({ "name": "shop-agent" })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_tenant_id" }));
}

#[tokio::test]
async fn missing_tenant_on_agent_create_returns_forbidden() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let token = auth_token_for_role(
        &state,
        tenant["id"].as_str().unwrap(),
        crate::repositories::UserRole::TenantAdmin,
        "other-admin",
    )
    .await;
    let tenant_id = "00000000-0000-0000-0000-000000000001";
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));
}

#[tokio::test]
async fn agent_create_returns_offline_record_and_audit_event() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "agent-admin",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["tenant_id"], tenant_id);
    assert_eq!(body["name"], "shop-agent");
    assert_eq!(body["status"], "offline");
    assert!(body["id"].as_str().is_some());
    assert!(body["created_at"].as_str().unwrap().ends_with('Z'));
    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(tenant_id).unwrap())
        .await
        .unwrap();
    assert!(events.iter().any(|event| event.action == "agent.create"));
}

#[tokio::test]
async fn empty_agent_name_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "empty-agent-admin",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json!({ "name": "" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "bad_request" }));
}

#[tokio::test]
async fn agent_list_returns_created_records() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "agent-list-admin",
    )
    .await;
    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [created] }));
}

#[tokio::test]
async fn tenant_admin_can_delete_offline_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = agent["tenant_id"].as_str().unwrap();
    let agent_id = agent["id"].as_str().unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, agent);

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [] }));

    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(tenant_id).unwrap())
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.delete")
        .expect("agent delete audit event");
    assert_eq!(event.target_id.as_deref(), Some(agent_id));
    let metadata = serde_json::from_str::<serde_json::Value>(&event.metadata_json).unwrap();
    assert_eq!(metadata["agent_name"], "shop-agent");
    assert_eq!(metadata["previous_status"], "offline");
}

#[tokio::test]
async fn agent_delete_rejects_online_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(agent["tenant_id"].as_str().unwrap()).unwrap();
    let agent_id = pandar_core::AgentId::parse(agent["id"].as_str().unwrap()).unwrap();
    state
        .agents()
        .update_connection(
            agent_id,
            pandar_core::AgentStatus::Online,
            Some("0.2.0"),
            "2026-06-20T01:00:00Z",
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "agent_online" }));
    assert!(state.agents().get(agent_id).await.unwrap().is_some());
}

#[tokio::test]
async fn viewer_cannot_delete_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = agent["tenant_id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::Viewer,
        "viewer-delete-agent",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!(
            "/api/v1/tenants/{}/agents/{}",
            tenant_id,
            agent["id"].as_str().unwrap()
        ),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn agent_delete_rejects_cross_tenant_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let other_tenant = state
        .tenants()
        .create("other-delete", "Other Delete")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &other_tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "cross-tenant-delete-agent",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!(
            "/api/v1/tenants/{}/agents/{}",
            other_tenant.id,
            agent["id"].as_str().unwrap()
        ),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "agent_not_found" }));
}

#[tokio::test]
async fn agent_delete_rejects_invalid_or_missing_agent() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "delete-missing-agent",
    )
    .await;

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/not-a-uuid"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_agent_id" }));

    let (status, body) = request_as(
        app,
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/agents/00000000-0000-0000-0000-000000000001"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "error": "agent_not_found" }));
}

#[tokio::test]
async fn invalid_tenant_id_on_agent_list_returns_bad_request() {
    let (status, body) = request(
        app().await,
        Method::GET,
        "/api/v1/tenants/not-a-uuid/agents",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_tenant_id" }));
}

#[tokio::test]
async fn missing_tenant_on_agent_list_returns_forbidden() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let token = auth_token_for_role(
        &state,
        tenant["id"].as_str().unwrap(),
        crate::repositories::UserRole::Viewer,
        "other-viewer",
    )
    .await;
    let tenant_id = "00000000-0000-0000-0000-000000000001";
    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "tenant_forbidden" }));
}

#[tokio::test]
async fn duplicate_agent_name_returns_conflict() {
    let state = state().await;
    let app = router(state.clone());
    let (_, tenant) = create_tenant_for_test(app.clone()).await;
    let tenant_id = tenant["id"].as_str().unwrap();
    let token = auth_token_for_role(
        &state,
        tenant_id,
        crate::repositories::UserRole::TenantAdmin,
        "duplicate-agent-admin",
    )
    .await;
    let uri = format!("/api/v1/tenants/{tenant_id}/agents");
    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &uri,
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = request_as(
        app,
        Method::POST,
        &uri,
        Some(json!({ "name": "shop-agent" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "error": "agent_name_exists" }));
}

async fn external_jwt_agent_list_response(token: &str) -> (StatusCode, serde_json::Value) {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();

    request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
        token,
    )
    .await
}
