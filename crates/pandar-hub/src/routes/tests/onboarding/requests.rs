use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct TenantCreateRequest<'a> {
    slug: &'a str,
    display_name: &'a str,
}

#[derive(Serialize)]
struct JoinLinkCreateRequest<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_constraint: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_uses: Option<i32>,
}

#[derive(Serialize)]
struct JoinLinkAcceptRequest<'a> {
    token: &'a str,
}

pub(super) fn tenant_create_body(slug: &str, display_name: &str) -> Option<Value> {
    Some(serde_json::to_value(TenantCreateRequest { slug, display_name }).unwrap())
}

pub(super) fn join_link_create_body(role: &str) -> Option<Value> {
    Some(
        serde_json::to_value(JoinLinkCreateRequest {
            role,
            email: None,
            email_constraint: None,
            expires_in_seconds: None,
            max_uses: None,
        })
        .unwrap(),
    )
}

pub(super) fn join_link_create_with_email_body(
    role: &str,
    email: &str,
    expires_in_seconds: i64,
    max_uses: i32,
) -> Option<Value> {
    Some(
        serde_json::to_value(JoinLinkCreateRequest {
            role,
            email: Some(email),
            email_constraint: None,
            expires_in_seconds: Some(expires_in_seconds),
            max_uses: Some(max_uses),
        })
        .unwrap(),
    )
}

pub(super) fn join_link_create_with_email_constraint_body(
    role: &str,
    email_constraint: &str,
) -> Option<Value> {
    Some(
        serde_json::to_value(JoinLinkCreateRequest {
            role,
            email: None,
            email_constraint: Some(email_constraint),
            expires_in_seconds: None,
            max_uses: None,
        })
        .unwrap(),
    )
}

pub(super) fn join_link_accept_body(token: &str) -> Option<Value> {
    Some(serde_json::to_value(JoinLinkAcceptRequest { token }).unwrap())
}
