use super::*;

#[tokio::test]
async fn printer_events_websocket_closes_immediately_when_process_epoch_changes() {
    let state = state().await;
    let tenant = state
        .tenants()
        .create("epoch-ws-acme", "Epoch WS Acme")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "epoch-ws-token",
    )
    .await;
    let http_addr = serve_http(router(state.clone())).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    state.printer_events().invalidate_epoch();

    assert_socket_closed_without_text(&mut ws, "epoch change").await;
}

#[tokio::test]
async fn epoch_change_after_serialization_closes_without_sending_stale_text() {
    let (state, tenant, token, http_addr) = epoch_window_fixture("after-serialization").await;
    let mut pause = crate::routes::printer_events::send_pause::install_after_serialization();
    let mut ws = connect_printer_events(http_addr, tenant.id, &token).await;
    state
        .printer_events()
        .publish_local(tenant.id, test_command_event("serialized"))
        .await;
    pause.wait_until_reached().await;

    state.printer_events().invalidate_epoch();
    pause.resume();

    assert_socket_closed_without_text(&mut ws, "epoch change after serialization").await;
}

#[tokio::test]
async fn epoch_change_cancels_a_blocked_websocket_flush() {
    let (state, tenant, token, http_addr) = epoch_window_fixture("blocked-flush").await;
    let mut pause = crate::routes::printer_events::send_pause::install_during_flush();
    let mut ws = connect_printer_events(http_addr, tenant.id, &token).await;
    state
        .printer_events()
        .publish_local(tenant.id, test_command_event("blocked"))
        .await;
    pause.wait_until_reached().await;

    state.printer_events().invalidate_epoch();

    assert_socket_closed_without_text(&mut ws, "epoch change during blocked flush").await;
    pause.resume();
}

#[tokio::test]
async fn printer_events_websocket_closes_immediately_when_tenant_receiver_lags() {
    let event_hub = crate::printer_events::PrinterEventHub::with_capacity_for_tests(1);
    let state = state()
        .await
        .with_printer_events_for_tests(event_hub.clone());
    let tenant = state
        .tenants()
        .create("lagged-ws-acme", "Lagged WS Acme")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "lagged-ws-token",
    )
    .await;
    let http_addr = serve_http(router(state)).await;
    let mut request = format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events",
        tenant.id
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    event_hub
        .publish_local_burst_for_tests(
            tenant.id,
            vec![test_command_event("first"), test_command_event("second")],
        )
        .await;

    let next = tokio::time::timeout(std::time::Duration::from_secs(1), ws.next())
        .await
        .expect("lagged receiver must close the websocket without waiting for another event");
    assert!(
        !matches!(next, Some(Ok(Message::Text(_)))),
        "lagged websocket must not resume from the newest buffered event"
    );
}

#[tokio::test]
async fn printer_events_cross_replica_ticket_safety_matrix() {
    let state = state().await;
    let subscriber = sibling_state(&state);
    let issuer = router(state.clone());
    let http_addr = serve_http(router(subscriber)).await;
    let tenant = state
        .tenants()
        .create("ticket-matrix-acme", "Ticket Matrix Acme")
        .await
        .unwrap();
    let other = state
        .tenants()
        .create("ticket-matrix-other", "Ticket Matrix Other")
        .await
        .unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-matrix-token",
    )
    .await;
    let other_token = auth_token_for_role(
        &state,
        &other.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "ticket-matrix-other-token",
    )
    .await;

    let ticket = issue_ticket(issuer.clone(), tenant.id, &token).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!(
        "ws://{http_addr}/api/v1/tenants/{}/printer-events?ticket={ticket}",
        tenant.id
    ))
    .await
    .unwrap();
    drop(ws);

    assert_ws_ticket_rejected(http_addr, tenant.id, &ticket).await;

    let wrong_tenant_ticket = issue_ticket(issuer.clone(), other.id, &other_token).await;
    assert_ws_ticket_rejected(http_addr, tenant.id, &wrong_tenant_ticket).await;

    let expired_ticket = "pandar_ws_expired_matrix";
    seed_expired_ticket(state.database(), tenant.id, expired_ticket).await;
    assert_ws_ticket_rejected(http_addr, tenant.id, expired_ticket).await;
}

#[tokio::test]
async fn printer_events_websocket_rejects_invalid_ticket_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/printer-events?ticket=not-a-ticket",
            tenant.id
        ),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
}

#[tokio::test]
async fn printer_events_websocket_rejects_wrong_tenant_ticket_before_upgrade() {
    let state = state().await;
    let app = router(state.clone());
    let tenant_a = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_b = state.tenants().create("beta", "Beta Labs").await.unwrap();
    let token = auth_token_for_role(
        &state,
        &tenant_a.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "tenant-a-ticket",
    )
    .await;
    let (status, body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/printer-events/tickets", tenant_a.id),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ticket = decode::<PrinterEventTicketResponse>(body).ticket;

    let (status, body) = request(
        app,
        Method::GET,
        &format!(
            "/api/v1/tenants/{}/printer-events?ticket={ticket}",
            tenant_b.id
        ),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
}

#[tokio::test]
async fn printer_events_unlinked_external_jwt_returns_forbidden_before_upgrade() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let token = jwt_for(
        "unlinked-ws-viewer",
        TEST_ISSUER,
        TEST_AUDIENCE,
        "test-key",
        300,
    );

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/printer-events", tenant.id),
        None,
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "tenant_forbidden");
}
