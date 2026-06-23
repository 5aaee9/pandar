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
        &format!("/api/v1/tenants/{tenant_id}/tenant-tokens"),
        Some(json!({ "name": "automation", "scopes": ["*"], "expires_at": null })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(token["tenant_token"]["name"], "automation");
    assert_eq!(token["tenant_token"]["scopes"], json!(["*"]));
    assert!(
        token["token"]
            .as_str()
            .unwrap()
            .starts_with("pandar_tenant_")
    );
    assert_eq!(token["tenant_token"]["revoked_at"], Value::Null);
    let plaintext_token = token["token"].as_str().unwrap();
    let token_id = token["tenant_token"]["id"].as_str().unwrap();

    let (status, tokens) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/tenant-tokens"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tokens["tenant_tokens"].as_array().unwrap().len(), 2);
    let listed = tokens["tenant_tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|token| token["id"] == token_id)
        .unwrap();
    assert!(listed.get("token").is_none());
    assert_eq!(listed["revoked_at"], Value::Null);

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
        &format!("/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["tenant_token"]["id"], token_id);
    assert!(revoked.get("token").is_none());
    assert!(revoked["tenant_token"]["revoked_at"].as_str().is_some());

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
            "tenant_token.create",
            "tenant_token.revoke"
        ]
    );
    let role_metadata =
        serde_json::from_str::<serde_json::Value>(&events[1].metadata_json).unwrap();
    assert_eq!(role_metadata["previous_role"], "operator");
    assert_eq!(role_metadata["new_role"], "viewer");
    assert!(role_metadata["tenant_token_id"].as_str().is_some());
    assert_eq!(role_metadata["tenant_token_scopes"], json!(["*"]));
    let identity_metadata =
        serde_json::from_str::<serde_json::Value>(&events[2].metadata_json).unwrap();
    assert_eq!(identity_metadata["provider"], "clerk");
    assert!(identity_metadata.get("subject").is_none());
    assert!(identity_metadata["tenant_token_id"].as_str().is_some());

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

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/users/{}/api-tokens", user.id),
        Some(json!({ "name": "" })),
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::GONE);
    assert_eq!(body, json!({ "error": "api_tokens_retired" }));
}
