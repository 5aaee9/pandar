use super::*;

#[tokio::test]
async fn tenant_admin_can_manage_users_identities_and_tokens() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);

    let (status, user) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        Some(json!({
            "email": "operator@example.test",
            "display_name": "Operator",
            "role": "operator"
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(user["tenant_id"], tenant_id);
    assert_eq!(user["email"], "operator@example.test");
    assert_eq!(user["role"], "operator");
    let user_id = user["id"].as_str().unwrap();

    let (status, users) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(users["users"].as_array().unwrap().len(), 2);

    let (status, updated) = request_as(
        app.clone(),
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/role"),
        Some(json!({ "role": "viewer" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["role"], "viewer");

    let (status, identity) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/identities"),
        Some(json!({ "provider": "clerk", "subject": "user_123" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(identity["provider"], "clerk");
    assert_eq!(identity["subject"], "user_123");

    let (status, identities) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/identities"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(identities, json!({ "identities": [identity] }));

    let (status, token) = request_as(
        app.clone(),
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens"),
        Some(json!({ "name": "automation" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(token["name"], "automation");
    assert!(token["token"].as_str().unwrap().starts_with("pandar_"));
    assert_eq!(token["revoked_at"], Value::Null);
    let plaintext_token = token["token"].as_str().unwrap();
    let token_id = token["id"].as_str().unwrap();

    let (status, tokens) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users/{user_id}/api-tokens"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tokens["api_tokens"].as_array().unwrap().len(), 1);
    assert!(tokens["api_tokens"][0].get("token").is_none());
    assert_eq!(tokens["api_tokens"][0]["revoked_at"], Value::Null);

    let (status, _) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        plaintext_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, revoked) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/api-tokens/{token_id}"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["id"], token_id);
    assert!(revoked["token"].is_null());
    assert!(revoked["revoked_at"].as_str().is_some());

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/agents"),
        None,
        plaintext_token,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let actions = events
        .iter()
        .map(|event| event.action.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            "user.create",
            "user.role_update",
            "user_identity.link",
            "api_token.create",
            "api_token.revoke"
        ]
    );
    assert_eq!(
        events[1].metadata_json,
        json!({ "previous_role": "operator", "new_role": "viewer" }).to_string()
    );

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        Some(json!({
            "email": "bad-role@example.test",
            "display_name": "Bad Role",
            "role": "admin"
        })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, json!({ "error": "invalid_user_role" }));
}

#[tokio::test]
async fn provisioning_mutations_reject_empty_required_strings() {
    let (state, app, tenant_id, admin_token) = admin_tenant().await;
    let tenant = state.tenants().list().await.unwrap().remove(0);
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "target@example.test",
            "Target User",
            crate::repositories::UserRole::Viewer,
        )
        .await
        .unwrap();

    for (uri, body) in [
        (
            format!("/api/v1/tenants/{tenant_id}/users"),
            json!({ "email": "", "display_name": "Target", "role": "viewer" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users"),
            json!({ "email": "empty-name@example.test", "display_name": "", "role": "viewer" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users"),
            json!({ "email": "empty-role@example.test", "display_name": "Target", "role": "" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/role", user.id),
            json!({ "role": "" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/identities", user.id),
            json!({ "provider": "", "subject": "subject" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/identities", user.id),
            json!({ "provider": "clerk", "subject": "" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/users/{}/api-tokens", user.id),
            json!({ "name": "" }),
        ),
        (
            format!("/api/v1/tenants/{tenant_id}/agent-pairings"),
            json!({ "name": "" }),
        ),
    ] {
        let method = if uri.ends_with("/role") {
            Method::PATCH
        } else {
            Method::POST
        };
        let (status, body) = request_as(app.clone(), method, &uri, Some(body), &admin_token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, json!({ "error": "bad_request" }));
    }
}
