use super::*;
use crate::repositories::UserRole;

#[tokio::test]
async fn tenant_admin_deletes_only_a_clearable_job() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("delete-route", "Delete Route")
        .await
        .unwrap();
    let other_tenant = state
        .tenants()
        .create("delete-route-other", "Delete Route Other")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let terminal =
        super::clear::create_job(&state, tenant.id, agent.id, &printer_id, "delete-terminal").await;
    state
        .jobs()
        .mark_print_sent(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_succeeded(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .apply_print_report(super::clear::report(
            tenant.id,
            agent.id,
            &printer_id,
            terminal.job.id,
            "FINISH",
        ))
        .await
        .unwrap();
    let active =
        super::clear::create_job(&state, tenant.id, agent.id, &printer_id, "delete-active").await;
    let operator =
        external_auth_token_for_role(&state, tenant.id, UserRole::Operator, "delete-operator")
            .await;
    let admin =
        external_auth_token_for_role(&state, tenant.id, UserRole::TenantAdmin, "delete-admin")
            .await;
    let other_admin = external_auth_token_for_role(
        &state,
        other_tenant.id,
        UserRole::TenantAdmin,
        "delete-other-admin",
    )
    .await;
    let terminal_uri = format!("/api/v1/tenants/{}/jobs/{}", tenant.id, terminal.job.id);

    let (status, body) =
        request_as(app.clone(), Method::DELETE, &terminal_uri, None, &operator).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");

    let response = raw_request_as(app.clone(), Method::DELETE, &terminal_uri, &admin).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        state
            .jobs()
            .get_for_tenant(tenant.id, terminal.job.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .jobs()
            .get_for_tenant(tenant.id, active.job.id)
            .await
            .unwrap()
            .is_some()
    );

    let active_uri = format!("/api/v1/tenants/{}/jobs/{}", tenant.id, active.job.id);
    let (status, body) = request_as(app.clone(), Method::DELETE, &active_uri, None, &admin).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode::<ErrorResponse>(body).error, "job_not_clearable");

    let wrong_tenant_uri = format!("/api/v1/tenants/{}/jobs/{}", other_tenant.id, active.job.id);
    let (status, body) = request_as(
        app.clone(),
        Method::DELETE,
        &wrong_tenant_uri,
        None,
        &other_admin,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(decode::<ErrorResponse>(body).error, "job_not_found");

    let invalid_uri = format!("/api/v1/tenants/{}/jobs/not-a-job", tenant.id);
    let (status, body) = request_as(app, Method::DELETE, &invalid_uri, None, &admin).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "invalid_job_id");

    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.delete")
        .unwrap();
    assert_eq!(event.target_type, "job");
    assert_eq!(
        event.target_id.as_deref(),
        Some(terminal.job.id.to_string().as_str())
    );
    assert!(event.metadata_json.contains("\"deleted_jobs\":1"));
    assert!(
        event
            .metadata_json
            .contains("\"artifact_id\":\"delete-terminal\"")
    );
    assert!(
        event
            .metadata_json
            .contains("\"artifact_filename\":\"plate.3mf\"")
    );
    assert!(
        event
            .metadata_json
            .contains("\"previous_dispatch_status\":\"succeeded\"")
    );
    assert!(
        event
            .metadata_json
            .contains("\"previous_print_status\":\"completed\"")
    );
}

#[tokio::test]
async fn all_scope_tenant_token_can_delete_a_clearable_job() {
    let state = state().await;
    let app = router(external_auth_state(state.clone()));
    let tenant = state
        .tenants()
        .create("delete-token-route", "Delete Token Route")
        .await
        .unwrap();
    let agent = state.agents().create(tenant.id, "agent").await.unwrap();
    let printer_id = insert_printer_fixture(state.database(), tenant.id, agent.id)
        .await
        .unwrap();
    let terminal = super::clear::create_job(
        &state,
        tenant.id,
        agent.id,
        &printer_id,
        "delete-token-terminal",
    )
    .await;
    state
        .jobs()
        .mark_print_sent(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .mark_print_succeeded(terminal.job.command_id, tenant.id, agent.id)
        .await
        .unwrap();
    state
        .jobs()
        .apply_print_report(super::clear::report(
            tenant.id,
            agent.id,
            &printer_id,
            terminal.job.id,
            "FINISH",
        ))
        .await
        .unwrap();
    let token = all_scope_tenant_token(&state, &tenant.id.to_string(), "delete-all-scope").await;
    let uri = format!("/api/v1/tenants/{}/jobs/{}", tenant.id, terminal.job.id);

    let response = raw_request_as(app, Method::DELETE, &uri, &token).await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let events = state
        .audit_events()
        .list_for_tenant(tenant.id)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.action == "job.delete")
        .unwrap();
    assert_eq!(event.actor_type, "tenant_token");
}
