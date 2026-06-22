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
