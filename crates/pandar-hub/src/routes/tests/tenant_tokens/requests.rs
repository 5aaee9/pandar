use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct TenantTokenCreateRequest<'a> {
    name: &'a str,
    scopes: &'a [&'a str],
    expires_at: Option<&'a str>,
}

#[derive(Serialize)]
struct TenantTokenCreateWithoutScopesRequest<'a> {
    name: &'a str,
    expires_at: Option<&'a str>,
}

#[derive(Serialize)]
struct TenantTokenRotateRequest<'a> {
    expires_at: Option<&'a str>,
}

#[derive(Serialize)]
struct AgentNameRequest<'a> {
    name: &'a str,
}

pub(super) fn tenant_token_create_body(
    name: &str,
    scopes: &[&str],
    expires_at: Option<&str>,
) -> Option<Value> {
    Some(
        serde_json::to_value(TenantTokenCreateRequest {
            name,
            scopes,
            expires_at,
        })
        .unwrap(),
    )
}

pub(super) fn tenant_token_create_without_scopes_body(
    name: &str,
    expires_at: Option<&str>,
) -> Option<Value> {
    Some(serde_json::to_value(TenantTokenCreateWithoutScopesRequest { name, expires_at }).unwrap())
}

pub(super) fn tenant_token_rotate_body(expires_at: Option<&str>) -> Option<Value> {
    Some(serde_json::to_value(TenantTokenRotateRequest { expires_at }).unwrap())
}

pub(super) fn agent_name_body(name: &str) -> Option<Value> {
    Some(serde_json::to_value(AgentNameRequest { name }).unwrap())
}
