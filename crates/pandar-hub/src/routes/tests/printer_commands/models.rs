use super::*;

#[tokio::test]
async fn web_and_mobile_controls_accept_x1c_p1s_and_a2l_models() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();

    for model in ["X1 Carbon", "P1S", "A2L"] {
        let printer_id = crate::repositories::test_helpers::insert_printer_fixture_with_model(
            state.database(),
            tenant_id,
            agent_id,
            Some(model),
        )
        .await
        .unwrap();

        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/printers/{printer_id}/controls"),
            printer_control_body(PrinterControlRequest::action("pause")),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{model}: {body}");
        let command = decode::<CommandResponse>(body);
        let payload: PrinterOperationPayload = serde_json::from_str(&command.payload_json).unwrap();
        assert_eq!(payload.operation.kind, "pause", "{model}");
    }

    assert_eq!(state.commands().count().await.unwrap(), 3);
}
