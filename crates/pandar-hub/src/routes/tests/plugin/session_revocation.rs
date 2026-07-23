use super::*;

#[derive(serde::Deserialize)]
struct PluginSessionRevokeAuditMetadata {
    name: String,
    tenant_token_id: String,
    tenant_token_scopes: Vec<String>,
}

#[tokio::test]
async fn plugin_session_revoke_is_idempotent_and_audited_once() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-session-revoke", "Plugin Session Revoke")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "session-revoke").await;
    let stored_token = state.auth().list_tenant_tokens(tenant.id).await.unwrap()[0].clone();
    let token_id = stored_token.id.clone();

    for _ in 0..2 {
        let response = raw_request_as(
            app.clone(),
            Method::DELETE,
            "/api/v1/plugin/session",
            &token,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );
    }

    let (status, body) =
        request_as(app, Method::GET, "/api/v1/plugin/printers", None, &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");

    let revoke_events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.action == "tenant_token.revoke")
        .collect::<Vec<_>>();
    assert_eq!(revoke_events.len(), 1);
    assert_eq!(revoke_events[0].actor_type, "plugin_token");
    assert_eq!(revoke_events[0].user_id, stored_token.created_by_user_id);
    assert_eq!(
        revoke_events[0].target_id.as_deref(),
        Some(token_id.as_str())
    );
    let metadata =
        serde_json::from_str::<PluginSessionRevokeAuditMetadata>(&revoke_events[0].metadata_json)
            .unwrap();
    assert_eq!(metadata.name, "session-revoke");
    assert_eq!(metadata.tenant_token_id, token_id);
    assert_eq!(metadata.tenant_token_scopes, ["plugin:studio"]);
    assert!(!revoke_events[0].metadata_json.contains(&token));
}

#[tokio::test]
async fn plugin_session_revoke_rejects_unknown_and_wrong_scope_without_side_effects() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-session-reject", "Plugin Session Reject")
        .await
        .unwrap();
    let wrong_scope =
        all_scope_tenant_token(&state, &tenant.id.to_string(), "session-reject").await;
    let mixed_scope = tenant_token_for_scopes(
        &state,
        &tenant.id.to_string(),
        "session-reject-mixed",
        vec![
            crate::repositories::TenantTokenScope::PluginStudio,
            crate::repositories::TenantTokenScope::All,
        ],
    )
    .await;

    for token in [
        "unknown-plugin-session-token",
        wrong_scope.as_str(),
        mixed_scope.as_str(),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::DELETE,
            "/api/v1/plugin/session",
            None,
            token,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
    }

    let stored = state.auth().list_tenant_tokens(tenant.id).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|token| token.revoked_at.is_none()));
    assert!(
        state
            .audit_events()
            .list_for_tenant(tenant.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn expired_plugin_session_can_self_revoke() {
    let state = state().await;
    let tenant = state
        .tenants()
        .create("plugin-session-expired", "Plugin Session Expired")
        .await
        .unwrap();
    let created = state
        .auth()
        .create_tenant_token_with_audit(
            tenant.id,
            "Expired Studio Session",
            vec![crate::repositories::TenantTokenScope::PluginStudio],
            Some("2000-01-01T00:00:00Z".to_owned()),
            crate::repositories::AuditActor::no_auth(),
        )
        .await
        .unwrap();
    assert!(
        state
            .auth()
            .authenticate_tenant_token(&created.plaintext_token)
            .await
            .unwrap()
            .is_none()
    );

    let response = raw_request_as(
        router(state.clone()),
        Method::DELETE,
        "/api/v1/plugin/session",
        &created.plaintext_token,
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let stored = state.auth().list_tenant_tokens(tenant.id).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert!(stored[0].revoked_at.is_some());
    assert_eq!(
        state
            .audit_events()
            .list_for_tenant(tenant.id)
            .await
            .unwrap()
            .into_iter()
            .filter(|event| event.action == "tenant_token.revoke")
            .count(),
        1
    );
}

#[tokio::test]
async fn concurrent_plugin_session_revoke_has_one_audit_winner() {
    let state = AppState::file_sqlite_for_tests()
        .await
        .unwrap()
        .with_bootstrap_token(TEST_BOOTSTRAP_TOKEN);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("plugin-session-race", "Plugin Session Race")
        .await
        .unwrap();
    let token = plugin_studio_tenant_token(&state, &tenant.id.to_string(), "session-race").await;

    let (first, second) = tokio::join!(
        raw_request_as(
            app.clone(),
            Method::DELETE,
            "/api/v1/plugin/session",
            &token,
        ),
        raw_request_as(app, Method::DELETE, "/api/v1/plugin/session", &token,),
    );

    assert_eq!(first.status(), StatusCode::NO_CONTENT);
    assert_eq!(second.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        state
            .audit_events()
            .list_for_tenant(tenant.id)
            .await
            .unwrap()
            .into_iter()
            .filter(|event| event.action == "tenant_token.revoke")
            .count(),
        1
    );
}

#[tokio::test]
async fn plugin_session_revoke_only_changes_the_token_derived_tenant() {
    let state = state().await;
    let app = router(state.clone());
    let first_tenant = state
        .tenants()
        .create("plugin-session-first", "Plugin Session First")
        .await
        .unwrap();
    let second_tenant = state
        .tenants()
        .create("plugin-session-second", "Plugin Session Second")
        .await
        .unwrap();
    let first =
        plugin_studio_tenant_token(&state, &first_tenant.id.to_string(), "session-first").await;
    let second =
        plugin_studio_tenant_token(&state, &second_tenant.id.to_string(), "session-second").await;

    let response = raw_request_as(
        app.clone(),
        Method::DELETE,
        "/api/v1/plugin/session",
        &first,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (status, _) = request_as(app, Method::GET, "/api/v1/plugin/printers", None, &second).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        state
            .audit_events()
            .list_for_tenant(first_tenant.id)
            .await
            .unwrap()
            .into_iter()
            .filter(|event| event.action == "tenant_token.revoke")
            .count(),
        1
    );
    assert!(
        state
            .audit_events()
            .list_for_tenant(second_tenant.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        state
            .auth()
            .list_tenant_tokens(second_tenant.id)
            .await
            .unwrap()[0]
            .revoked_at,
        None
    );
}
