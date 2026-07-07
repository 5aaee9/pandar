use super::*;
use serde::{Deserialize, Serialize};

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
    let body: AgentsResponse = serde_json::from_value(body).unwrap();
    assert_eq!(body.agents.len(), 0);
}

#[tokio::test]
async fn no_auth_allows_bootstrap_routes_without_bootstrap_token() {
    let state = raw_state().await.with_no_auth_for_tests(true);
    let app = router(state);

    let (status, body) = request(
        app,
        Method::POST,
        "/api/v1/tenants",
        Some(
            serde_json::to_value(CreateTenantRequest {
                slug: "no-auth-tenant",
                display_name: "No Auth Tenant",
            })
            .unwrap(),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let body: TenantResponse = serde_json::from_value(body).unwrap();
    assert_eq!(body.slug, "no-auth-tenant");
    assert_eq!(body.display_name, "No Auth Tenant");
}

#[derive(Deserialize)]
struct AgentsResponse {
    agents: Vec<AgentResponse>,
}

#[derive(Deserialize)]
struct AgentResponse {}

#[derive(Serialize)]
struct CreateTenantRequest<'a> {
    slug: &'a str,
    display_name: &'a str,
}

#[derive(Deserialize)]
struct TenantResponse {
    slug: String,
    display_name: String,
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
        Some(serde_json::to_value(CreateAgentRequest { name: "shop-agent" }).unwrap()),
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

#[derive(Serialize)]
struct CreateAgentRequest<'a> {
    name: &'a str,
}
