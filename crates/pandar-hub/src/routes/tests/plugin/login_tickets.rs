use super::*;

#[tokio::test]
async fn plugin_login_ticket_creation_requires_external_operator() {
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
    let operator = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "plugin-operator",
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

    let (status, operator_body) =
        request_as(app.clone(), Method::POST, &uri, body(), &operator).await;
    assert_eq!(status, StatusCode::CREATED);
    let operator_body = decode::<LoginTicketResponse>(operator_body);
    assert!(operator_body.ticket.starts_with("pandar_plugin_ticket_"));
    assert!(operator_body.expires_at.ends_with('Z'));
    assert_eq!(
        operator_body.redirect_url,
        "http://localhost:4100/callback?state=abc"
    );

    for denied in [&viewer, &all, &empty, &agent_register, &plugin_studio] {
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
            &operator,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_redirect_url");
    }
}

#[tokio::test]
async fn mobile_session_observes_current_user_role() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("mobile-current-role", "Mobile Current Role")
        .await
        .unwrap();
    let admin = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "mobile-current-role-admin",
    )
    .await;
    let user = state
        .auth()
        .list_users_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|user| user.email == "mobile-current-role-admin@example.test")
        .unwrap();
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
        mobile_login_ticket_body("zip.iptables.pandar.android://auth/callback"),
        &admin,
    )
    .await;
    let created = decode::<LoginTicketResponse>(created);
    let (_, exchanged) = request(
        app.clone(),
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        mobile_ticket_exchange_body(&created.ticket),
    )
    .await;
    let session = decode::<ExchangeLoginTicketResponse>(exchanged).token;

    state
        .auth()
        .update_user_role(tenant.id, &user.id, crate::repositories::UserRole::Viewer)
        .await
        .unwrap();
    let (status, _) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("role-downgrade", &[]),
        &session,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_tickets_reject_cross_kind_exchange_and_invalid_pkce() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("ticket-kind", "Ticket Kind")
        .await
        .unwrap();
    let operator = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "ticket-kind-operator",
    )
    .await;

    let (_, plugin_body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id),
        plugin_login_ticket_body("http://localhost:4100/callback"),
        &operator,
    )
    .await;
    let plugin_ticket = decode::<LoginTicketResponse>(plugin_body).ticket;
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        mobile_ticket_exchange_body(&plugin_ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&plugin_ticket),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, mobile_body) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
        mobile_login_ticket_body("zip.iptables.pandar.android://auth/callback"),
        &operator,
    )
    .await;
    let mobile_ticket = decode::<LoginTicketResponse>(mobile_body).ticket;
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&mobile_ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        Some(serde_json::json!({
            "ticket": mobile_ticket,
            "code_verifier": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = request(
        app,
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        mobile_ticket_exchange_body(&mobile_ticket),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
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
    let operator = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "plugin-exchange-operator",
    )
    .await;
    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id),
        plugin_login_ticket_body("http://127.0.0.1:4100/callback"),
        &operator,
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
    assert_eq!(exchanged.profile.user_name, "External Test User [pandar]");
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
async fn concurrent_plugin_login_ticket_exchange_succeeds_once() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("plugin-concurrent-exchange", "Plugin Concurrent Exchange")
        .await
        .unwrap();
    let operator = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "plugin-concurrent-exchange-operator",
    )
    .await;
    let (_, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/plugin/login-tickets", tenant.id),
        plugin_login_ticket_body("http://localhost:4100/callback"),
        &operator,
    )
    .await;
    let ticket = decode::<LoginTicketResponse>(created).ticket;

    let first = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&ticket),
    );
    let second = request(
        app,
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        plugin_ticket_exchange_body(&ticket),
    );
    let ((first_status, _), (second_status, _)) = tokio::join!(first, second);
    let mut statuses = [first_status, second_status];
    statuses.sort();

    assert_eq!(statuses, [StatusCode::OK, StatusCode::UNAUTHORIZED]);
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
    let operator = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "mobile-exchange-operator",
    )
    .await;

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
        mobile_login_ticket_body("zip.iptables.pandar.android://auth/callback"),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, created) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
        mobile_login_ticket_body("zip.iptables.pandar.android://auth/callback"),
        &operator,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created = decode::<LoginTicketResponse>(created);
    assert!(created.ticket.starts_with("pandar_mobile_ticket_"));
    assert_eq!(
        created.redirect_url,
        "zip.iptables.pandar.android://auth/callback"
    );

    let (status, exchanged) = request(
        app.clone(),
        Method::POST,
        "/api/v1/mobile/login-tickets/exchange",
        mobile_ticket_exchange_body(&created.ticket),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let exchanged = decode::<ExchangeLoginTicketResponse>(exchanged);
    assert!(exchanged.token.starts_with("pandar_mobile_"));
    assert_eq!(exchanged.profile.tenant_id, tenant.id.to_string());
    assert_eq!(exchanged.profile.tenant_name, "Mobile Exchange");

    let (status, _) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{}/tenant-tokens", tenant.id),
        tenant_token_create_body("forbidden", &[]),
        &exchanged.token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

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
        mobile_ticket_exchange_body(&created.ticket),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_plugin_ticket");

    for redirect_url in [
        "http://localhost:4100/callback",
        "zip.iptables.pandar.android://auth/callback?state=abc",
        "zip.iptables.pandar.android:/oauth2redirect",
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{}/mobile/login-tickets", tenant.id),
            mobile_login_ticket_body(redirect_url),
            &operator,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_redirect_url");
    }
}
