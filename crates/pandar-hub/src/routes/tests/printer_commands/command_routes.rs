use super::*;

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
    let result_json = printer_discovery_result_json();
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
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.id, command.id.to_string());
    assert_eq!(body.result_json.as_deref(), Some(result_json.as_str()));

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
    assert_eq!(decode::<ErrorResponse>(body).error, "command_not_found");
}

#[tokio::test]
async fn invalid_command_id_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/commands/not-a-uuid"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_command_id");
}

#[tokio::test]
async fn invalid_agent_id_on_refresh_returns_bad_request() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, _, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/not-a-uuid/refresh-printers"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_agent_id");
}
