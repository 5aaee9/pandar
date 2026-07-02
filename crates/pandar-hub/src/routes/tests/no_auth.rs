use super::*;

#[tokio::test]
async fn no_auth_allows_tenant_read_without_bearer_token() {
    let state = state().await.with_no_auth_for_tests(true);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("no-auth-read", "No Auth Read")
        .await
        .unwrap();

    let (status, body) = request(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "agents": [] }));
}

#[tokio::test]
async fn no_auth_allows_bootstrap_routes_without_bootstrap_token() {
    let state = raw_state().await.with_no_auth_for_tests(true);
    let app = router(state);

    let (status, body) = request(
        app,
        Method::POST,
        "/api/v1/tenants",
        Some(json!({
            "slug": "no-auth-tenant",
            "display_name": "No Auth Tenant"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["slug"], "no-auth-tenant");
    assert_eq!(body["display_name"], "No Auth Tenant");
}

#[tokio::test]
async fn no_auth_mutations_record_no_auth_audit_actor() {
    let state = state().await.with_no_auth_for_tests(true);
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("no-auth-audit", "No Auth Audit")
        .await
        .unwrap();

    let (status, _) = request(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        Some(json!({ "name": "shop-agent" })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "agent.create")
        .expect("agent create audit event");
    assert_eq!(event.actor_type, "no_auth");
    assert_eq!(event.user_id, None);
}
