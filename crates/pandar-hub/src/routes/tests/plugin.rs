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
    let body = || {
        Some(json!({
            "redirect_url": "http://localhost:4100/callback?state=abc"
        }))
    };

    let (status, viewer_body) = request_as(app.clone(), Method::POST, &uri, body(), &viewer).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(
        viewer_body["ticket"]
            .as_str()
            .unwrap()
            .starts_with("pandar_plugin_ticket_")
    );
    assert!(viewer_body["expires_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(
        viewer_body["redirect_url"],
        "http://localhost:4100/callback?state=abc"
    );

    let (status, _) = request_as(app.clone(), Method::POST, &uri, body(), &all).await;
    assert_eq!(status, StatusCode::CREATED);
    for denied in [&empty, &agent_register, &plugin_studio] {
        let (status, body) = request_as(app.clone(), Method::POST, &uri, body(), denied).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({ "error": "role_forbidden" }));
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
            Some(json!({ "redirect_url": redirect_url })),
            &viewer,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "invalid_redirect_url" }));
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
        Some(json!({ "redirect_url": "http://127.0.0.1:4100/callback" })),
        &viewer,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let ticket = created["ticket"].as_str().unwrap();

    let (status, exchanged) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        Some(json!({ "ticket": ticket })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        exchanged["token"]
            .as_str()
            .unwrap()
            .starts_with("pandar_plugin_")
    );
    assert!(exchanged["expires_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(exchanged["profile"]["tenant_id"], tenant.id.to_string());
    assert_eq!(exchanged["profile"]["tenant_name"], "Plugin Exchange");

    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/v1/plugin/login-tickets/exchange",
        Some(json!({ "ticket": ticket })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_plugin_ticket" }));

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
        Some(json!({ "ticket": expired.plaintext_ticket })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_plugin_ticket" }));
}

#[tokio::test]
async fn plugin_routes_only_accept_plugin_studio_tokens() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-auth", "Plugin Auth")
        .await
        .unwrap();
    let plugin = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "studio").await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "all").await;
    let empty = read_only_tenant_token(&state, &tenant.id.to_string(), "empty").await;
    let mixed = all_and_plugin_studio_tenant_token(&state, &tenant.id.to_string(), "mixed").await;

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        "/api/v1/plugin/printers",
        None,
        &plugin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "printers": [] }));

    for denied in [&all, &empty, &mixed] {
        let (status, body) = request_as(
            app.clone(),
            Method::GET,
            "/api/v1/plugin/printers",
            None,
            denied,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({ "error": "role_forbidden" }));
    }
}

#[tokio::test]
async fn plugin_print_returns_job_shape_and_records_plugin_actor_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print", "Plugin Print")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "print-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "queued");
    assert_eq!(body["message"], Value::Null);
    assert!(body["task_id"].as_str().is_some());
    assert!(body["command_id"].as_str().is_some());
    assert_eq!(body["pandar_job_id"], body["task_id"]);
    assert!(body.get("print").is_none());
    assert!(body.get("artifact").is_none());
    assert!(body.get("printer_id").is_none());

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.create")
        .unwrap();
    let metadata: Value = serde_json::from_str(&event.metadata_json).unwrap();
    assert_eq!(event.actor_type, "plugin_token");
    assert!(metadata["tenant_token_id"].as_str().is_some());
    assert_eq!(metadata["tenant_token_scopes"], json!(["plugin:studio"]));
    assert!(metadata.get("token").is_none());
    assert!(metadata.get("ticket").is_none());
}

#[tokio::test]
async fn plugin_print_wakes_agent_on_sibling_instance() {
    let state = state().await;
    let sibling = sibling_state(&state);
    let _control_plane = start_control_plane(sibling.clone()).await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-print-sibling", "Plugin Print Sibling")
        .await
        .unwrap();
    let token =
        plugin_studio_tenant_token(&state, &tenant.id.to_string(), "sibling-print-plugin").await;
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let (wake_sender, mut wake_receiver) = tokio::sync::mpsc::channel(1);
    let (close_sender, _) = tokio::sync::mpsc::channel(1);
    sibling
        .sessions()
        .register(crate::sessions::AgentSession {
            token: crate::sessions::SessionToken::new(),
            tenant_id: tenant.id,
            agent_id: agent.id,
            name: "agent".to_owned(),
            version: "test".to_owned(),
            connected_at: pandar_core::created_at_now(),
            last_heartbeat_at: pandar_core::created_at_now(),
            wake_sender,
            close_sender,
        })
        .await;

    let (status, body) = multipart_request_as(
        app,
        Method::POST,
        "/api/v1/plugin/prints",
        multipart_print_body(
            Some(&printer_id),
            Some(("plugin plate.3mf", "model/3mf", b"abc")),
            1,
        ),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["status"], "queued");
    tokio::time::timeout(std::time::Duration::from_secs(1), wake_receiver.recv())
        .await
        .expect("sibling agent should be woken")
        .expect("wake channel should stay open");
}

#[tokio::test]
async fn audit_events_route_authorizes_paginates_filters_and_redacts_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("audit-plugin", "Audit Plugin")
        .await
        .unwrap();
    let admin = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "audit-admin",
    )
    .await;
    let viewer = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "audit-viewer",
    )
    .await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "audit-all").await;
    insert_audit_fixture(
        &state,
        tenant.id,
        "first.action",
        "2026-06-20T00:00:00Z",
        json!({
            "safe": "keep",
            "subject": "external-subject",
            "plaintext_token": "secret",
            "ticket": "ticket",
            "plaintext_ticket": "ticket",
            "nested": {
                "credential_hash": "hash",
                "provider_subject": "external-subject",
                "ticket_hash": "hash",
                "token_hash": "hash",
                "ok": true
            },
            "headers": { "Authorization": "Bearer secret" },
            "artifact_storage_path": "/tmp/secret"
        }),
    )
    .await;
    insert_audit_fixture(
        &state,
        tenant.id,
        "second.action",
        "2026-06-21T00:00:00Z",
        json!({ "safe": "second" }),
    )
    .await;

    let uri = format!("/api/v1/tenants/{}/audit-events?limit=1", tenant.id);
    let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &admin).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["audit_events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["action"], "second.action");

    let uri = format!(
        "/api/v1/tenants/{}/audit-events?before=2026-06-21T00:00:00Z&action=first.action",
        tenant.id
    );
    let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &all).await;
    assert_eq!(status, StatusCode::OK);
    let metadata = &body["audit_events"][0]["metadata"];
    assert_eq!(metadata["safe"], "keep");
    assert_eq!(metadata["nested"], json!({ "ok": true }));
    assert!(metadata.get("subject").is_none());
    assert!(metadata.get("plaintext_token").is_none());
    assert!(metadata.get("ticket").is_none());
    assert!(metadata.get("plaintext_ticket").is_none());
    assert!(metadata["headers"].get("Authorization").is_none());
    assert!(metadata.get("artifact_storage_path").is_none());

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/audit-events?limit=0", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_limit" }));

    let (status, body) = request_as(app, Method::GET, &uri, None, &viewer).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({ "error": "role_forbidden" }));
}

#[tokio::test]
async fn audit_events_route_falls_back_to_empty_metadata_for_invalid_persisted_json() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("audit-invalid", "Audit Invalid")
        .await
        .unwrap();
    let admin = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "invalid-audit-admin",
    )
    .await;
    insert_raw_audit_fixture(
        &state,
        tenant.id,
        "invalid.metadata",
        "2026-06-20T00:00:00Z",
        "{not-json",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/audit-events", tenant.id),
        None,
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["audit_events"][0]["metadata"], json!({}));
}

async fn insert_audit_fixture(
    state: &AppState,
    tenant_id: TenantId,
    action: &str,
    created_at: &str,
    metadata: Value,
) {
    insert_raw_audit_fixture(state, tenant_id, action, created_at, &metadata.to_string()).await;
}

async fn insert_raw_audit_fixture(
    state: &AppState,
    tenant_id: TenantId,
    action: &str,
    created_at: &str,
    metadata_json: &str,
) {
    sqlx::query(
        "INSERT INTO audit_events (id, tenant_id, actor_type, user_id, action, target_type, target_id, metadata_json, created_at)
         VALUES (?1, ?2, 'user', NULL, ?3, 'fixture', NULL, ?4, ?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(tenant_id.to_string())
    .bind(action)
    .bind(metadata_json)
    .bind(created_at)
    .execute(sqlite_pool(state))
    .await
    .unwrap();
}

fn sqlite_pool(state: &AppState) -> &sqlx::SqlitePool {
    let crate::db::Database::Sqlite(pool) = state.database() else {
        panic!("expected SQLite database");
    };
    pool
}
