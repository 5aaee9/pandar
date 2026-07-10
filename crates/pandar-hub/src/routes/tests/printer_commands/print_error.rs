use super::*;

#[tokio::test]
async fn tenant_printer_control_rejects_native_print_error_action() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
        state.database(),
        tenant_id,
        agent_id,
        Some("A1"),
    )
    .await
    .unwrap();

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
        Some(serde_json::json!({
            "action": "handle_print_error",
            "error_action": "resume",
            "print_error": 83_918_929,
            "printer_job_id": "job-7",
            "sequence_id": 20_042
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        decode::<ErrorResponse>(body).error,
        "invalid_printer_control"
    );
    assert_eq!(state.commands().count().await.unwrap(), 0);
}
