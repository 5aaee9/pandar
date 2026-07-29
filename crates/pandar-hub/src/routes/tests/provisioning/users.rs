use super::*;

#[derive(Debug, serde::Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, serde::Deserialize)]
struct UserEntry {
    id: String,
    email: String,
    role: String,
}

#[derive(Debug, serde::Deserialize)]
struct UserListBody {
    users: Vec<UserEntry>,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
}

async fn list_users(app: &Router, tenant_id: &str, token: &str) -> Vec<UserEntry> {
    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{tenant_id}/users"),
        None,
        token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    decode::<UserListBody>(body).users
}

#[tokio::test]
async fn admin_can_remove_another_member() {
    let state = bootstrap_state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_id = tenant.id.to_string();
    let admin_token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "admin-subject",
    )
    .await;
    let member = state
        .auth()
        .create_user(
            tenant.id,
            "member@example.test",
            "Member",
            crate::repositories::UserRole::Viewer,
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/users/{}", member.id),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let removed = decode::<UserEntry>(body);
    assert_eq!(removed.id, member.id);
    assert_eq!(removed.email, "member@example.test");

    let remaining = list_users(&app, &tenant_id, &admin_token).await;
    assert_eq!(remaining.len(), 1);
    assert!(remaining.iter().all(|user| user.id != member.id));
}

#[tokio::test]
async fn admin_cannot_remove_themselves() {
    let state = bootstrap_state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let tenant_id = tenant.id.to_string();
    let admin_token = external_auth_token_for_role(
        &state,
        tenant.id,
        crate::repositories::UserRole::TenantAdmin,
        "self-admin",
    )
    .await;
    let admin = list_users(&app, &tenant_id, &admin_token)
        .await
        .into_iter()
        .find(|user| user.email == "self-admin@example.test")
        .unwrap();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/users/{}", admin.id),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode::<ErrorResponse>(body).error, "cannot_remove_self");

    let remaining = list_users(&app, &tenant_id, &admin_token).await;
    assert_eq!(remaining.len(), 1);
}

#[tokio::test]
async fn last_tenant_admin_cannot_be_removed() {
    let (_state, app, tenant_id, admin_token) = admin_tenant().await;
    let users = list_users(&app, &tenant_id, &admin_token).await;
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].role, "tenant_admin");

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/users/{}", users[0].id),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode::<ErrorResponse>(body).error, "last_tenant_admin");

    assert_eq!(list_users(&app, &tenant_id, &admin_token).await.len(), 1);
}

#[tokio::test]
async fn removing_missing_user_returns_not_found() {
    let (_state, app, tenant_id, admin_token) = admin_tenant().await;
    let missing_user_id = uuid::Uuid::new_v4().to_string();

    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &format!("/api/v1/tenants/{tenant_id}/users/{missing_user_id}"),
        None,
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "user_not_found");
}
