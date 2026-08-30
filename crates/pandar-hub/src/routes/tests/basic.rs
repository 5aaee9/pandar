use super::*;

#[tokio::test]
async fn health_check_reports_ok() {
    let (status, body) = request(app().await, Method::GET, "/healthz", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode::<HealthResponse>(body).status, "ok");
}

#[derive(Debug, serde::Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, serde::Deserialize)]
struct AuthStatusResponse {
    external_auth: ExternalAuthStatus,
}

#[derive(Debug, serde::Deserialize)]
struct ExternalAuthStatus {
    enabled: bool,
    ready: bool,
}

#[tokio::test]
async fn public_auth_status_reports_external_auth_configuration() {
    let (status, body) = request(
        router(external_auth_state(state().await)),
        Method::GET,
        "/api/v1/auth/status",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let external_auth = decode::<AuthStatusResponse>(body).external_auth;
    assert!(external_auth.enabled);
    assert!(external_auth.ready);
}

#[tokio::test]
async fn public_auth_status_reports_disabled_external_auth() {
    let (status, body) = request(
        router(state().await),
        Method::GET,
        "/api/v1/auth/status",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let external_auth = decode::<AuthStatusResponse>(body).external_auth;
    assert!(!external_auth.enabled);
    assert!(external_auth.ready);
}

#[tokio::test]
async fn public_openapi_contract_exposes_generated_client_schemas() {
    let (status, body) = request(
        router(state().await),
        Method::GET,
        "/api/v1/openapi.json",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"]["/api/v1/tenants/{tenant_id}/printers"].is_object());
    assert_eq!(
        body["components"]["schemas"]["CommandStatus"]["enum"],
        serde_json::json!([
            "queued",
            "sent",
            "acknowledged",
            "succeeded",
            "failed",
            "cancelled"
        ])
    );
    for path in [
        "/api/v1/onboarding/tenants",
        "/api/v1/tenants/{tenant_id}/tenant-tokens",
        "/api/v1/tenants/{tenant_id}/tenant-tokens/{token_id}/rotate",
        "/api/v1/tenants/{tenant_id}/join-links",
        "/api/v1/tenants/{tenant_id}/agent-pairings",
        "/api/v1/tenants/{tenant_id}/plugin/login-tickets",
        "/api/v1/tenants/{tenant_id}/mobile/login-tickets",
        "/api/v1/tenants/{tenant_id}/jobs/{job_id}/retry-dispatch",
        "/api/v1/tenants/{tenant_id}/jobs/{job_id}/reprint",
    ] {
        assert!(body["paths"][path]["post"]["responses"]["201"].is_object());
        assert_eq!(
            body["paths"][path]["post"]["responses"]["default"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/ErrorResponse"
        );
    }
    for path in [
        "/api/v1/tenants/{tenant_id}/users",
        "/api/v1/tenants/{tenant_id}/join-links",
        "/api/v1/tenants/{tenant_id}/tenant-tokens",
        "/api/v1/tenants/{tenant_id}/audit-events",
        "/api/v1/tenants/{tenant_id}/printers/{printer_id}",
    ] {
        assert!(body["paths"][path]["get"]["responses"]["200"].is_object());
    }
}

#[derive(Debug, serde::Deserialize)]
struct ErrorResponse {
    error: String,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    decode_json(value)
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
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_auth_token");
}
