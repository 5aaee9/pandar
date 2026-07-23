use super::*;

#[tokio::test]
async fn audit_events_route_authorizes_paginates_filters_and_redacts_metadata() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("audit-plugin", "Audit Plugin")
        .await
        .unwrap();
    let admin = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "audit-admin",
    )
    .await;
    let viewer = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::Viewer,
        "audit-viewer",
    )
    .await;
    let all = all_scope_tenant_token(&state, &tenant.id.to_string(), "audit-all").await;
    insert_audit_fixture(
        &state,
        tenant.id,
        "first.action",
        "2026-06-20T00:00:00Z",
        redacted_audit_metadata_fixture(),
    )
    .await;
    insert_audit_fixture(
        &state,
        tenant.id,
        "second.action",
        "2026-06-21T00:00:00Z",
        safe_audit_metadata_fixture("second"),
    )
    .await;

    let uri = format!("/api/v1/tenants/{}/audit-events?limit=1", tenant.id);
    let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &admin).await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<AuditEventListResponse<IgnoredAny>>(body);
    assert_eq!(body.audit_events.len(), 1);
    assert_eq!(body.audit_events[0].action, "second.action");

    let uri = format!(
        "/api/v1/tenants/{}/audit-events?before=2026-06-21T00:00:00Z&action=first.action",
        tenant.id
    );
    let (status, body) = request_as(app.clone(), Method::GET, &uri, None, &all).await;
    assert_eq!(status, StatusCode::OK);
    let body = decode::<AuditEventListResponse<RedactedAuditMetadata>>(body);
    let metadata = &body.audit_events[0].metadata;
    assert_eq!(metadata.safe, "keep");
    assert_eq!(metadata.nested, RedactedNestedAuditMetadata { ok: true });
    assert_eq!(metadata.subject, None);
    assert_eq!(metadata.plaintext_token, None);
    assert_eq!(metadata.ticket, None);
    assert_eq!(metadata.plaintext_ticket, None);
    assert_eq!(metadata.headers.authorization, None);
    assert_eq!(metadata.artifact_storage_path, None);

    let (status, body) = request_as(
        app.clone(),
        Method::GET,
        &format!("/api/v1/tenants/{}/audit-events?limit=0", tenant.id),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_limit");

    let (status, body) = request_as(app, Method::GET, &uri, None, &viewer).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
}

#[tokio::test]
async fn audit_events_route_falls_back_to_empty_metadata_for_invalid_persisted_json() {
    let state = state().await;
    let app = router(state.clone());
    let tenant = state
        .tenants()
        .create("audit-invalid", "Audit Invalid")
        .await
        .unwrap();
    let admin = auth_token_for_role(
        &state,
        &tenant.id.to_string(),
        crate::repositories::UserRole::TenantAdmin,
        "invalid-audit-admin",
    )
    .await;
    insert_raw_audit_fixture(
        &state,
        tenant.id,
        "invalid.metadata",
        "2026-06-20T00:00:00Z",
        "{not-json",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::GET,
        &format!("/api/v1/tenants/{}/audit-events", tenant.id),
        None,
        &admin,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<AuditEventListResponse<EmptyAuditMetadata>>(body);
    assert_eq!(body.audit_events.len(), 1);
}
