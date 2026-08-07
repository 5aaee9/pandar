use super::*;

#[test]
fn grpc_hub_command_from_record_rejects_persisted_link_printer_replay() {
    let command = CommandRecord::from_parts(CommandRecordParts {
        id: CommandId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        printer_id: None,
        kind: "link_printer".to_string(),
        status: "sent".to_string(),
        payload_json:
            r#"{"printer_type":"BambuLab","host":"192.0.2.10","access_code":"[redacted]"}"#
                .to_string(),
        result_json: None,
        error: None,
        created_at: "2026-07-01T00:00:00Z".to_string(),
        updated_at: "2026-07-01T00:00:00Z".to_string(),
    })
    .unwrap();

    let err = hub_command_from_record(command).unwrap_err();

    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(
        err.message(),
        "link printer command requires live secret dispatch"
    );
}

#[tokio::test]
async fn grpc_link_printer_failed_result_redacts_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            format!("validation failed for access_code={access_code}"),
            String::new(),
        ),
        None,
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert!(!stored.error.unwrap().contains(access_code));
}

#[tokio::test]
async fn grpc_link_printer_result_json_redacts_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    let result_json = format!(r#"{{"access_code":"{access_code}","status":"rejected"}}"#);

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(false, String::new(), result_json),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let stored_result = stored.result_json.unwrap();
    assert!(!stored_result.contains(access_code));
    assert_eq!(
        redacted_result::<RedactedLinkPrinterResult>(&stored_result),
        RedactedLinkPrinterResult {
            access_code: "[redacted]".to_owned(),
            status: "rejected".to_owned(),
        }
    );
}

#[tokio::test]
async fn grpc_link_printer_failed_result_preserves_error_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    let error = "validate runtime printer: printer rejected the access code";
    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            error.to_owned(),
            r#"{"type":"printer_link_error","error_code":"invalid_access_code"}"#.to_owned(),
        ),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert_eq!(stored.error.as_deref(), Some(error));
    let stored_result = stored.result_json.unwrap();
    assert_eq!(
        redacted_result::<RedactedLinkPrinterFailure>(&stored_result),
        RedactedLinkPrinterFailure {
            kind: "printer_link_error".to_owned(),
            error_code: "invalid_access_code".to_owned(),
        }
    );
}

#[tokio::test]
async fn grpc_link_printer_numeric_result_json_redacts_digit_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "12345678";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            String::new(),
            r#"{"echoed":12345678,"status":"rejected"}"#.to_owned(),
        ),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let stored_result = stored.result_json.unwrap();
    assert!(!stored_result.contains(access_code));
    assert_eq!(
        redacted_result::<RedactedNumericLinkPrinterResult>(&stored_result),
        RedactedNumericLinkPrinterResult {
            echoed: "[redacted]".to_owned(),
            status: "rejected".to_owned(),
        }
    );
}

#[tokio::test]
async fn grpc_link_printer_result_json_redacts_access_code_object_key() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();

    handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            String::new(),
            format!(r#"{{"{access_code}":"rejected","status":"failed"}}"#),
        ),
        Some(access_code),
    )
    .await
    .unwrap();

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    let stored_result = stored.result_json.unwrap();
    assert!(!stored_result.contains(access_code));
    assert_eq!(
        redacted_result::<RedactedSecretKeyLinkPrinterResult>(&stored_result),
        RedactedSecretKeyLinkPrinterResult {
            secret_key: "rejected".to_owned(),
            status: "failed".to_owned(),
        }
    );
}

#[tokio::test]
async fn grpc_late_link_printer_result_logs_without_access_code() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let command = state
        .commands()
        .create_link_printer_sent_with_audit(
            tenant_id,
            agent_id,
            LinkPrinterPayload {
                printer_type: "BambuLab".to_owned(),
                host: "192.0.2.10".to_owned(),
                access_code: access_code.to_owned(),
                name: None,
            },
            test_audit_actor(),
        )
        .await
        .unwrap();
    state
        .commands()
        .mark_succeeded(command.id, tenant_id, agent_id)
        .await
        .unwrap();

    let _guard = tracing::subscriber::set_default(subscriber);
    let err = handle_result_and_job(
        &state,
        tenant_id,
        agent_id,
        command.id,
        command_result_payload(
            false,
            format!("validation failed for access_code={access_code}"),
            String::new(),
        ),
        None,
    )
    .await
    .unwrap_err();
    tracing::error!(error = %crate::redaction::redact_secrets(&format!("{err:#}")), "failed to process late link printer result");
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("failed to process late link printer result"));
    assert!(!captured.contains(access_code));
}
