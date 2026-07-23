use super::*;

#[tokio::test]
async fn printer_events_invalid_tenant_returns_bad_request_before_upgrade() {
    let (status, body) = request(
        app().await,
        Method::GET,
        "/api/v1/tenants/not-a-uuid/printer-events",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_tenant_id");
}

#[tokio::test]
async fn printer_events_missing_tenant_returns_not_found_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ws-viewer",
    )
    .await;
    let tenant_id = TenantId::new();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/printer-events"),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_forbidden");
}

#[tokio::test]
async fn printer_events_websocket_accepts_linked_viewer_jwt() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "linked-ws-viewer",
    )
    .await;
    let http_addr = serve_http(app).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());

    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    drop(ws);
}

#[tokio::test]
async fn printer_events_websocket_accepts_no_auth_without_ticket() {
    let state = state().await.with_no_auth_for_tests(true);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("no-auth-ws", "No Auth WS")
        .await
        .unwrap();
    let http_addr = serve_http(app).await;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    ))
    .await
    .unwrap();

    drop(ws);
}

#[tokio::test]
async fn printer_events_ticket_requires_linked_viewer() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let linked = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "ticket-viewer",
    )
    .await;
    let unlinked = jwt_for(
        "unlinked-ticket-viewer",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = request(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "missing_auth_token");

    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &unlinked,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_forbidden");

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &linked,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<PrinterEventTicketResponse>(body);
    assert!(!body.ticket.is_empty());
    assert!(!body.expires_at.is_empty());
}

#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_once() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(app.clone()).await;
    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);

    let err = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("401") || message.contains("Unauthorized"),
        "unexpected reused-ticket error: {message}"
    );
}

#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_from_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let app = router(sibling);
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sibling-ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(app.clone()).await;
    let (status, body) = request_as(
        router(state.clone()),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);
}

#[tokio::test]
async fn printer_events_websocket_accepts_browser_ticket_from_separate_sqlite_connection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let database_url = format!(
        "sqlite://{}",
        temp_dir.path().join("pandar-ticket-test.db").display()
    );
    let issuer_storage = JobStorageConfig::new(
        temp_dir.path().join("issuer-spool"),
        DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let subscriber_storage = JobStorageConfig::new(
        temp_dir.path().join("subscriber-spool"),
        DEFAULT_MAX_ARTIFACT_BYTES,
    )
    .unwrap();
    let issuer = AppState::connect_with_config_values(
        database_url.clone(),
        issuer_storage,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let subscriber = AppState::connect_with_config_values(
        database_url,
        subscriber_storage,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let tenant = issuer
        .tenants()
        .create("sqlite-file-acme", "SQLite File Acme")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &issuer,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "sqlite-file-ticket-ws-token",
    )
    .await;
    let http_addr = serve_http(router(subscriber)).await;
    let (status, body) = request_as(
        router(issuer),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);
}
