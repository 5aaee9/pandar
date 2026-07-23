use super::*;

#[tokio::test]
async fn plugin_login_ticket_creation_enforces_external_viewer_or_all_tenant_token() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("plugin-acme", "Plugin Acme")
        .await
        .unwrap();
    let viewer = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "plugin-viewer",
    )
    .await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "plugin-all").await;
    let empty = read_only_tenant_token(&state, &tenant.id.to_string(), "plugin-empty").await;
    let agent_register =
        agent_register_tenant_token(&state, &tenant.id.to_string(), "plugin-agent").await;
    let plugin_studio =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "plugin-studio").await;
    let uri = format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id);
    let body = || plugin_login_ticket_body("http://localhost:4100/callback?state=abc");

    let (status, viewer_body) = request_as(app.clone(), Method::POST, &uri, body(), &viewer).await;
    assert_eq!(status, StatusCode::CREATED);
    let viewer_body = decode::<LoginTicketResponse>(viewer_body);
    assert!(viewer_body.ticket.starts_with("pandar_plugin_ticket_"));
    assert!(viewer_body.expires_at.ends_with('Z'));
    assert_eq!(
        viewer_body.redirect_url,
        "http://localhost:4100/callback?state=abc"
    );

    let (status, _) = request_as(app.clone(), Method::POST, &uri, body(), &all).await;
    assert_eq!(status, StatusCode::CREATED);
    for denied in [&empty, &agent_register, &plugin_studio] {
        let (status, body) = request_as(app.clone(), Method::POST, &uri, body(), denied).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    }

    for redirect_url in [
        "https://localhost:4100/callback",
        "http://example.test:4100/callback",
        "http://localhost/callback",
        "http://user:pass@localhost:4100/callback",
        "http://localhost:4100/callback#fragment",
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &uri,
            plugin_login_ticket_body(redirect_url),
            &viewer,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_redirect_url");
    }
}

#[tokio::test]
async fn plugin_login_ticket_exchange_is_unauthenticated_one_use_and_rejects_expired() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("plugin-exchange", "Plugin Exchange")
        .await
        .unwrap();
    let viewer = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "plugin-exchange-viewer",
    )
    .await;
    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id),
        plugin_login_ticket_body("http://127.0.0.1:4100/callback"),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket = decode::<LoginTicketResponse>(created).ticket;

    let (status, exchanged) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&ticket),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let exchanged = decode::<ExchangeLoginTicketResponse>(exchanged);
    assert!(exchanged.token.starts_with("pandar_plugin_"));
    assert!(exchanged.expires_at.ends_with('Z'));
    assert_eq!(exchanged.profile.tenant_id, tenant.id.to_string());
    assert_eq!(exchanged.profile.tenant_name, "Plugin Exchange");

    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_plugin_ticket");

    let expired = state
        .auth()
        .create_plugin_login_ticket_with_audit(
            tenant.id,
            None,
            "http://localhost:4100/expired",
            "2026-01-01T00:00:00Z".to_owned(),
            crate::repositories::AuditActor {
                actor_type: "system".to_owned(),
                user_id: None,
                metadata: None,
            },
        )
        .await
        .unwrap();
    sqlx::query("UPDATE plugin_login_tickets SET expires_at = ?2 WHERE id = ?1")
        .bind(&expired.ticket.id)
        .bind("2026-01-01T00:00:00Z")
        .execute(sqlite_pool(&state))
        .await
        .unwrap();
    let (status, body) = request(
        app,
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&expired.plaintext_ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_plugin_ticket");
}

#[tokio::test]
async fn mobile_login_ticket_exchange_returns_tenant_token_for_android_callback() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("mobile-exchange", "Mobile Exchange")
        .await
        .unwrap();
    let viewer = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "mobile-exchange-viewer",
    )
    .await;

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
        plugin_login_ticket_body("zip.iptables.pandar.android:/auth/callback"),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created = decode::<LoginTicketResponse>(created);
    assert!(created.ticket.starts_with("pandar_plugin_ticket_"));
    assert_eq!(
        created.redirect_url,
        "zip.iptables.pandar.android:/auth/callback"
    );

    let (status, exchanged) = request(
        app.clone(),
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        plugin_ticket_exchange_body(&created.ticket),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let exchanged = decode::<ExchangeLoginTicketResponse>(exchanged);
    assert!(exchanged.token.starts_with("pandar_mobile_"));
    assert_eq!(exchanged.profile.tenant_id, tenant.id.to_string());
    assert_eq!(exchanged.profile.tenant_name, "Mobile Exchange");

    let (status, _) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
        &exchanged.token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        plugin_ticket_exchange_body(&created.ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_plugin_ticket");

    for redirect_url in [
        "http://localhost:4100/callback",
        "zip.iptables.pandar.android:/auth/callback?state=abc",
        "zip.iptables.pandar.android:/oauth2redirect",
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
            plugin_login_ticket_body(redirect_url),
            &viewer,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_redirect_url");
    }
}
