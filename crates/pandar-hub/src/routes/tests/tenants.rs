use axum::http::Method;
use serde_json::json;

use super::*;

async fn renamed_tenant_app() -> (AppState, Router, String) {
    let state = bootstrap_state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    (state, app, tenant.id.to_string())
}

#[tokio::test]
async fn admin_user_can_rename_tenant() {
    let (state, app, tenant_id) = renamed_tenant_app().await;
    let admin_token = external_auth_token_for_role(
        &state,
        TenantId::parse(&tenant_id).unwrap(),
        crate::repositories::UserRole::TenantAdmin,
        "rename-admin",
    )
    .await;

    let (status, body) = request_as(
        app.clone(),
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}"),
        Some(json!({ "display_name": "Acme Studio" })),
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["display_name"], "Acme Studio");
    assert_eq!(body["slug"], "acme");

    let stored = state
        .tenants()
        .get(TenantId::parse(&tenant_id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.display_name, "Acme Studio");

    let events = state
        .audit_events()
        .list_for_tenant(TenantId::parse(&tenant_id).unwrap())
        .await
        .unwrap();
    let rename = events
        .iter()
        .find(|event| event.action == "tenant.rename")
        .expect("rename audit event recorded");
    assert!(rename.metadata_json.contains("Acme Labs"));
    assert!(rename.metadata_json.contains("Acme Studio"));
}

#[tokio::test]
async fn empty_display_name_returns_bad_request() {
    let (state, app, tenant_id) = renamed_tenant_app().await;
    let admin_token = external_auth_token_for_role(
        &state,
        TenantId::parse(&tenant_id).unwrap(),
        crate::repositories::UserRole::TenantAdmin,
        "rename-admin-empty",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}"),
        Some(json!({ "display_name": "   " })),
        &admin_token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn viewer_cannot_rename_tenant() {
    let (state, app, tenant_id) = renamed_tenant_app().await;
    let viewer_token = external_auth_token_for_role(
        &state,
        TenantId::parse(&tenant_id).unwrap(),
        crate::repositories::UserRole::Viewer,
        "rename-viewer",
    )
    .await;

    let (status, _body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}"),
        Some(json!({ "display_name": "Acme Studio" })),
        &viewer_token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tenant_token_cannot_rename_tenant() {
    let (state, app, tenant_id) = renamed_tenant_app().await;
    let token = all_scope_tenant_token(&state, &tenant_id, "rename-token").await;

    let (status, _body) = request_as(
        app,
        Method::PATCH,
        &format!("/api/v1/tenants/{tenant_id}"),
        Some(json!({ "display_name": "Acme Studio" })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}
