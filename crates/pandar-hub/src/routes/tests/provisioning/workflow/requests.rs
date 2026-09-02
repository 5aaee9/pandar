use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct UserRoleRequest<'a> {
    role: &'a str,
}

#[derive(Serialize)]
struct TenantTokenCreateRequest<'a> {
    name: &'a str,
    scopes: &'a [&'a str],
    expires_at: Option<&'a str>,
}

#[derive(Serialize)]
struct AgentNameRequest<'a> {
    name: &'a str,
}

pub(super) fn user_role_value(role: &str) -> Value {
    serde_json::to_value(UserRoleRequest { role }).unwrap()
}

pub(super) fn tenant_token_create_value(
    name: &str,
    scopes: &[&str],
    expires_at: Option<&str>,
) -> Value {
    serde_json::to_value(TenantTokenCreateRequest {
        name,
        scopes,
        expires_at,
    })
    .unwrap()
}

pub(super) fn agent_name_value(name: &str) -> Value {
    serde_json::to_value(AgentNameRequest { name }).unwrap()
}
