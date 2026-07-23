use super::*;

#[tokio::test]
async fn grpc_late_link_printer_result_stream_keeps_session_and_pending_redacted() {
    let logs = CapturedLogs::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(logs.writer())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
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
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command.id,
            link_printer_hub_command(command.id, access_code),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();
    state
        .commands()
        .mark_failed(
            command.id,
            tenant_id,
            agent_id,
            "stale cleanup failed first",
        )
        .await
        .unwrap();

    sender
        .send(Ok(result_event(
            tenant_id,
            agent_id,
            command.id,
            false,
            format!("printer rejected {access_code}"),
            format!(r#"{{"message":"{access_code}"}}"#),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(state.sessions().get(agent_id).await.unwrap().token, token);
    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert_eq!(stored.error.as_deref(), Some("stale cleanup failed first"));
    assert!(
        state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );
    drop(_guard);

    let captured = logs.to_string();
    assert!(captured.contains("ignored late live printer link command result"));
    assert!(!captured.contains(access_code));
}

#[tokio::test]
async fn grpc_link_printer_stream_result_redacts_standalone_access_code() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
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
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command.id,
            link_printer_hub_command(command.id, access_code),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(result_event(
            tenant_id,
            agent_id,
            command.id,
            false,
            format!("printer rejected {access_code}"),
            format!(r#"{{"message":"{access_code}"}}"#),
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let stored = state
        .commands()
        .get_for_tenant(tenant_id, command.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, CommandStatus::Failed);
    assert!(!stored.error.unwrap().contains(access_code));
    assert!(!stored.result_json.unwrap().contains(access_code));
    assert!(
        !state
            .sessions()
            .pending_live_command_ids()
            .await
            .contains(&command.id)
    );
}

#[tokio::test]
async fn grpc_link_printer_rejected_ack_redacts_pending_secret_from_error() {
    let state = fixture_state().await;
    let (tenant_id, agent_id) = tenant_agent(&state).await;
    let access_code = "SECRET-LINK-CODE";
    let (mut stream, sender) = connect_live(&state, vec![hello_event(tenant_id, agent_id)])
        .await
        .unwrap();
    let token = state.sessions().get(agent_id).await.unwrap().token;
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
        .sessions()
        .try_dispatch_live_command(
            tenant_id,
            agent_id,
            token,
            command.id,
            link_printer_hub_command(command.id, access_code),
        )
        .await
        .unwrap();
    let _ = stream.next().await.unwrap().unwrap();

    sender
        .send(Ok(failed_ack_event(
            tenant_id,
            agent_id,
            command.id,
            access_code,
        )))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

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
async fn grpc_link_printer_result_without_pending_secret_redacts_untrusted_strings() {
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
            format!("printer rejected {access_code}"),
            format!(r#"{{"message":"{access_code}","status":"rejected"}}"#),
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
    assert!(!stored.error.unwrap().contains(access_code));
    let result_json = stored.result_json.unwrap();
    assert!(!result_json.contains(access_code));
    assert_eq!(
        redacted_result::<RedactedUnknownLinkPrinterResult>(&result_json),
        RedactedUnknownLinkPrinterResult {
            first: "[redacted]".to_owned(),
            second: "[redacted]".to_owned(),
        }
    );
}

#[tokio::test]
async fn grpc_link_printer_result_without_pending_secret_redacts_numeric_values() {
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
    let result_json = stored.result_json.unwrap();
    assert!(!result_json.contains(access_code));
    assert_eq!(
        redacted_result::<RedactedUnknownLinkPrinterResult>(&result_json),
        RedactedUnknownLinkPrinterResult {
            first: "[redacted]".to_owned(),
            second: "[redacted]".to_owned(),
        }
    );
}

#[tokio::test]
async fn grpc_link_printer_result_without_pending_secret_redacts_numeric_object_key() {
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
            r#"{"12345678":"rejected","status":"failed"}"#.to_owned(),
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
    let result_json = stored.result_json.unwrap();
    assert!(!result_json.contains(access_code));
    assert_eq!(
        redacted_result::<RedactedUnknownLinkPrinterResult>(&result_json),
        RedactedUnknownLinkPrinterResult {
            first: "[redacted]".to_owned(),
            second: "[redacted]".to_owned(),
        }
    );
}
