use super::*;

#[tokio::test]
async fn link_printer_requires_operator_role() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, _) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;
    let token = auth_token_for_role(
        &state,
        &tenant_id,
        crate::repositories::UserRole::Viewer,
        "viewer-link-printer-token",
    )
    .await;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode::<ErrorResponse>(body).error, "role_forbidden");
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_missing_local_session_without_command_row() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body("SECRET-LINK-CODE")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode::<ErrorResponse>(body).error, "agent_not_connected");
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_missing_local_session_does_not_log_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;
    let access_code = "SECRET-LINK-CODE";

    let _ = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;
    drop(_guard);

    assert!(!logs.to_string().contains(access_code));
}

#[tokio::test]
async fn link_printer_direct_sends_secret_but_persists_only_redacted_payload() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "link_printer");
    assert_eq!(body.status, "sent");
    assert!(!body.payload_json.contains(access_code));
    assert!(
        !body
            .error
            .as_deref()
            .unwrap_or_default()
            .contains(access_code)
    );

    let sent = command_receiver.recv().await.unwrap().unwrap();
    match sent.command.unwrap() {
        hub_command::Command::LinkPrinter(command) => {
            assert_eq!(command.printer_type, "BambuLab");
            assert_eq!(command.host, "192.168.2.10");
            assert_eq!(command.access_code, access_code);
            assert_eq!(command.name, "Office X1C");
        }
        other => panic!("expected link printer command, got {other:?}"),
    }

    let payload: LinkPrinterPayload = serde_json::from_str(&body.payload_json).unwrap();
    assert_eq!(payload.printer_type, "BambuLab");
    assert_eq!(payload.host, "192.168.2.10");
    assert_eq!(payload.access_code, "[redacted]");
    assert_eq!(payload.name, "Office X1C");
    assert_eq!(payload.serial_number, None);
    assert_eq!(payload.model, None);
}

#[tokio::test]
async fn link_printer_maps_absent_or_blank_optional_name_to_empty_proto_string() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (command_sender, mut command_receiver) = tokio::sync::mpsc::channel(1);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;

    for body in [
        link_printer_value("BambuLab", "192.168.2.10", "SECRET-LINK-CODE", None),
        link_printer_value("BambuLab", "192.168.2.11", "SECRET-LINK-CODE", Some("   ")),
    ] {
        let (status, response) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let response = decode::<CommandResponse>(response);
        let sent = command_receiver.recv().await.unwrap().unwrap();
        match sent.command.unwrap() {
            hub_command::Command::LinkPrinter(command) => {
                assert_eq!(command.name, "");
            }
            other => panic!("expected link printer command, got {other:?}"),
        }
        assert_eq!(response.status, "sent");
    }
}

#[tokio::test]
async fn link_printer_marks_command_failed_when_live_channel_closed_after_row_creation() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = TenantId::parse(&decode::<TenantResponse>(tenant).id).unwrap();
    let agent_id = AgentId::parse(&decode::<AgentResponse>(agent).id).unwrap();
    let (command_sender, command_receiver) = tokio::sync::mpsc::channel(1);
    drop(command_receiver);
    register_route_test_session(&state, tenant_id, agent_id, command_sender).await;
    let access_code = "SECRET-LINK-CODE";

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        Some(link_printer_body(access_code)),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let body = decode::<CommandResponse>(body);
    assert_eq!(body.kind, "link_printer");
    assert_eq!(body.status, "failed");
    assert_eq!(
        body.error.as_deref(),
        Some("agent command channel unavailable before printer link completed")
    );
    assert!(!body.payload_json.contains(access_code));
    assert!(
        !body
            .error
            .as_deref()
            .unwrap_or_default()
            .contains(access_code)
    );
    assert_eq!(state.commands().count().await.unwrap(), 1);
    let command_id = pandar_core::CommandId::parse(&body.id).unwrap();
    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, pandar_core::CommandStatus::Failed);
    assert_eq!(
        stored.error.as_deref(),
        Some("agent command channel unavailable before printer link completed")
    );
    assert!(
        !state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command_id)
    );
}

#[tokio::test]
async fn link_printer_rejects_blank_required_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    for body in [
        link_printer_value("BambuLab", "", "SECRET-LINK-CODE", None),
        link_printer_value("BambuLab", "192.168.2.10", "", None),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(body),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    }

    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_invalid_type_host_and_legacy_metadata_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    for request in [
        link_printer_value("", "192.168.2.10", "SECRET-LINK-CODE", None),
        link_printer_value("Other", "192.168.2.10", "SECRET-LINK-CODE", None),
        link_printer_value("BambuLab", "printer.local", "SECRET-LINK-CODE", None),
        link_printer_with_serial_number_value(
            "BambuLab",
            "192.168.2.10",
            "SECRET-LINK-CODE",
            "SERIAL123",
        ),
        link_printer_with_model_value("BambuLab", "192.168.2.10", "SECRET-LINK-CODE", "X1 Carbon"),
    ] {
        let (status, body) = request_as(
            app.clone(),
            Method::POST,
            &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
            Some(request),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    }
    assert_eq!(state.commands().count().await.unwrap(), 0);
}

#[tokio::test]
async fn link_printer_rejects_unknown_fields() {
    let state = state().await;
    let app = router(state.clone());
    let (tenant, agent, token) = tenant_and_agent(&state, app.clone()).await;
    let tenant_id = decode::<TenantResponse>(tenant).id;
    let agent_id = decode::<AgentResponse>(agent).id;

    let (status, body) = request_as(
        app,
        Method::POST,
        &format!("/api/v1/tenants/{tenant_id}/agents/{agent_id}/link-printer"),
        link_printer_with_unexpected_field_body("BambuLab", "192.168.2.10", "SECRET-LINK-CODE"),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(decode::<ErrorResponse>(body).error, "bad_request");
    assert_eq!(state.commands().count().await.unwrap(), 0);
}
