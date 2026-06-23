use super::*;

#[tokio::test]
async fn operator_and_viewer_cannot_use_provisioning_routes() {
    let state = bootstrap_state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_id = tenant.id.to_string();
    let operator_token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Operator,
        "operator-token",
    )
    .await;
    let viewer_token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::Viewer,
        "viewer-token",
    )
    .await;
    let target_user = state
        .auth()
        .create_user(
            tenant.id,
            "target@example.test",
            "Target User",
            crate::repositories::UserRole::Viewer,
        )
        .await
        .unwrap();
    let target_user_id = target_user.id;

    for token in [&operator_token, &viewer_token] {
        for (method, uri, body) in [
            (
                Method::GET,
                format!("/api/v1/tenants/{tenant_id}/users"),
                None,
            ),
            (
                Method::POST,
                format!("/api/v1/tenants/{tenant_id}/users"),
                Some(json!({
                    "email": "blocked@example.test",
                    "display_name": "Blocked",
                    "role": "viewer"
                })),
            ),
            (
                Method::PATCH,
                format!("/api/v1/tenants/{tenant_id}/users/{target_user_id}/role"),
                Some(json!({ "role": "operator" })),
            ),
            (
                Method::GET,
                format!("/api/v1/tenants/{tenant_id}/users/{target_user_id}/identities"),
                None,
            ),
            (
                Method::POST,
                format!("/api/v1/tenants/{tenant_id}/users/{target_user_id}/identities"),
                Some(json!({ "provider": "clerk", "subject": "blocked" })),
            ),
            (
                Method::POST,
                format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
                Some(json!({ "name": "blocked-agent" })),
            ),
        ] {
            let (status, body) = request_as(app.clone(), method, &uri, body, token).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body, json!({ "error": "role_forbidden" }));
        }
    }

    for (method, uri, body) in [
        (
            Method::GET,
            format!("/api/v1/tenants/{tenant_id}/users/{target_user_id}/api-tokens"),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/tenants/{tenant_id}/users/{target_user_id}/api-tokens"),
            Some(json!({ "name": "blocked" })),
        ),
        (
            Method::DELETE,
            format!("/api/v1/tenants/{tenant_id}/api-tokens/missing-token"),
            None,
        ),
    ] {
        let (status, body) = request_as(app.clone(), method, &uri, body, &viewer_token).await;
        assert_eq!(status, StatusCode::GONE);
        assert_eq!(body, json!({ "error": "api_tokens_retired" }));
    }
}

#[tokio::test]
async fn tenant_admin_cannot_manage_other_tenant_users() {
    let state = bootstrap_state().await;
    let app = router(state.clone());
    let tenant_a = state.tenants().create("acme-a", "Acme A").await.unwrap();
    let tenant_b = state.tenants().create("acme-b", "Acme B").await.unwrap();
    let tenant_a_id = tenant_a.id.to_string();
    let tenant_b_id = tenant_b.id.to_string();
    let admin_a_token = auth_token_for_role(&state, &tenant_a_id, admin(), "admin-a-token").await;
    let admin_b_token = auth_token_for_role(&state, &tenant_b_id, admin(), "admin-b-token").await;
    let user_b = state
        .auth()
        .create_user(
            tenant_b.id,
            "operator-b@example.test",
            "Operator B",
            crate::repositories::UserRole::Operator,
        )
        .await
        .unwrap();
    for (method, uri, body) in [(
        Method::GET,
        format!("/api/v1/tenants/{tenant_b_id}/users"),
        None,
    )] {
        let (status, body) = request_as(app.clone(), method, &uri, body, &admin_a_token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({ "error": "tenant_forbidden" }));
    }

    let (status, _) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_b_id}/users"),
        None,
        &admin_b_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for (method, uri) in [
        (
            Method::POST,
            format!(
                "/api/v1/tenants/{tenant_b_id}/users/{}/api-tokens",
                user_b.id
            ),
        ),
        (
            Method::DELETE,
            format!("/api/v1/tenants/{tenant_b_id}/api-tokens/missing-token"),
        ),
    ] {
        let (status, body) = request_as(
            app.clone(),
            method,
            &uri,
            Some(json!({ "name": "retired" })),
            &admin_a_token,
        )
        .await;
        assert_eq!(status, StatusCode::GONE);
        assert_eq!(body, json!({ "error": "api_tokens_retired" }));
    }
}

#[tokio::test]
async fn tenant_admin_gets_not_found_for_missing_user_nested_lists() {
    let (_state, app, tenant_id, admin_token) = admin_tenant().await;
    let missing_user_id = uuid::Uuid::new_v4().to_string();

    for uri in [format!(
        "/api/v1/tenants/{tenant_id}/users/{missing_user_id}/identities"
    )] {
        let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &admin_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, json!({ "error": "user_not_found" }));
    }

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users/{missing_user_id}/api-tokens"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(body, json!({ "error": "api_tokens_retired" }));
}
