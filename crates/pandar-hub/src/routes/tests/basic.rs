use super::*;

#[tokio::test]
async fn health_check_reports_ok() {
    let (status, body) = request(app().await, Method::GET, "/healthz", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn retired_api_token_auth_is_rejected_when_external_auth_is_configured() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state.tenants().create("acme", "Acme Labs").await.unwrap();
    let user = state
        .auth()
        .create_user(
            tenant.id,
            "api-token-user@example.test",
            "API Token User",
            crate::repositories::UserRole::Viewer,
        )
        .await
        .unwrap();
    state
        .auth()
        .create_api_token(
            tenant.id,
            &user.id,
            "retired-api-token",
            "retired-api-token",
        )
        .await
        .unwrap();

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/agents", tenant.id),
        None,
        "retired-api-token",
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "error": "invalid_auth_token" }));
}
