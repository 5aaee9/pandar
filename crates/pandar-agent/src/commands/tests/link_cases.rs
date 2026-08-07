use super::*;

fn assert_link_failure(event: AgentEvent, command_id: &str, needle: &str, error_code: &str) {
    match event.event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(!result.success);
            assert!(result.error.contains(needle), "{}", result.error);
            assert_eq!(link_failure(&result.result_json).error_code, error_code);
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[tokio::test]
async fn command_failure_redacts_access_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "ACCESS-CODE-UNIQUE";
    let gateway = FakeGateway::fail_with_access_code(access_code);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        refresh_command(command_id.clone()),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(!result.success);
            assert!(!result.error.contains(access_code));
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert_eq!(result.result_json, "");
        }
        other => panic!("expected command result, got {other:?}"),
    }
}

#[tokio::test]
async fn link_printer_emits_ack_snapshot_and_success_without_access_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::success(snapshot(
        "SERIAL123",
        "Office X1C",
        Some("X1 Carbon"),
        "READY",
    ));
    let (sender, mut receiver) = mpsc::channel(3);
    let access_code = "SECRET-LINK-CODE";

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), access_code),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL123",
        "Office X1C",
        "X1 Carbon",
        "READY",
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(result.success);
            assert!(!result.result_json.contains(access_code));
            assert_eq!(
                link_result(&result.result_json),
                TestPrinterLinkResult {
                    kind: "printer_link".to_owned(),
                    serial_number: "SERIAL123".to_owned(),
                    host: "192.0.2.10".to_owned(),
                    name: "Office X1C".to_owned(),
                    model: "X1 Carbon".to_owned(),
                    status: "READY".to_owned(),
                }
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(gateway.linked_endpoints().await.len(), 1);
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn link_printer_gateway_snapshot_precedes_firmware_events_without_duplicate() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::success_with_firmware(snapshot(
        "SERIAL123",
        "Office X1C",
        Some("X1 Carbon"),
        "READY",
    ));
    let (sender, mut receiver) = mpsc::channel(8);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL123",
        "Office X1C",
        "X1 Carbon",
        "READY",
    );
    let invalidation = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareInvalidated(invalidation) = invalidation.event.unwrap()
    else {
        panic!("expected firmware invalidation after linked printer snapshot");
    };
    let modules = receiver.recv().await.unwrap();
    let agent_event::Event::PrinterFirmwareModulesSnapshot(modules) = modules.event.unwrap() else {
        panic!("expected firmware modules after link invalidation");
    };
    assert_eq!(modules.generation, invalidation.generation);
    assert_eq!(modules.module_revision, 1);
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
        }
        other => panic!("expected command result without a duplicate snapshot, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn link_printer_fails_when_discovery_does_not_find_host() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.11",
        Some("OTHER"),
        Some("A1 Mini"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(
        receiver.recv().await.unwrap(),
        &command_id,
        "could not discover printer at 192.0.2.10",
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_uses_direct_host_discovery_when_multicast_misses_host() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result_with_direct_host(
        Vec::new(),
        Some(discovered_printer(
            "192.0.2.10",
            Some("SERIAL123"),
            Some("X1 Carbon"),
        )),
    );
    let (sender, mut receiver) = mpsc::channel(3);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_snapshot(
        receiver.recv().await.unwrap(),
        "SERIAL123",
        "Office X1C",
        "X1 Carbon",
        "READY",
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert_eq!(result.command_id, command_id);
            assert!(result.success);
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert_eq!(gateway.linked_endpoints().await.len(), 1);
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn link_printer_fails_when_discovered_printer_has_no_serial() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.10",
        None,
        Some("X1 Carbon"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), "SECRET-LINK-CODE"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_link_failure(
        receiver.recv().await.unwrap(),
        &command_id,
        "printer serial could not be discovered for 192.0.2.10",
        "printer_not_found",
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_rejects_unsupported_type_without_discovery() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = LinkGateway::discovery_result(vec![discovered_printer(
        "192.0.2.10",
        Some("SERIAL123"),
        Some("X1 Carbon"),
    )]);
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command_with_type(command_id.clone(), "SECRET-LINK-CODE", "Other"),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    assert_failure_contains(
        receiver.recv().await.unwrap(),
        &command_id,
        "unsupported printer type Other",
    );
    assert!(receiver.recv().await.is_none());
    assert!(gateway.linked_endpoints().await.is_empty());
}

#[tokio::test]
async fn link_printer_failure_redacts_access_code_from_result_error() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "SECRET-LINK-CODE";
    let gateway = LinkGateway::failure_with_error(
        access_code,
        anyhow::anyhow!("Certificate is not valid for {access_code}")
            .context("complete Bambu MQTT TLS handshake"),
    );
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), access_code),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(!result.success);
            assert!(result.error.contains("validate runtime printer"));
            assert!(result.error.contains("complete Bambu MQTT TLS handshake"));
            assert!(result.error.contains("Certificate is not valid"));
            assert!(result.error.contains("[REDACTED_ACCESS_CODE]"));
            assert!(!result.error.contains(access_code));
            assert_eq!(
                link_failure(&result.result_json),
                TestPrinterLinkFailure {
                    kind: "printer_link_error".to_owned(),
                    error_code: "link_failed".to_owned(),
                }
            );
            assert!(!result.result_json.contains(access_code));
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn link_printer_maps_rejected_access_code_to_error_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "SECRET-LINK-CODE";
    let gateway = LinkGateway::failure_with_error(
        access_code,
        anyhow::Error::new(rumqttc::ConnectionError::ConnectionRefused(
            rumqttc::ConnectReturnCode::BadUserNamePassword,
        ))
        .context("poll rumqttc event loop"),
    );
    let (sender, mut receiver) = mpsc::channel(2);

    handle_command_with_gateway(
        &config,
        &gateway,
        &sender,
        link_printer_command(command_id.clone(), access_code),
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(
        receiver.recv().await.unwrap(),
        ack_event(&config, &command_id)
    );
    match receiver.recv().await.unwrap().event.unwrap() {
        agent_event::Event::CommandResult(result) => {
            assert!(!result.success);
            assert!(!result.error.contains(access_code));
            assert_eq!(
                link_failure(&result.result_json).error_code,
                "invalid_access_code"
            );
        }
        other => panic!("expected command result, got {other:?}"),
    }
    assert!(receiver.recv().await.is_none());
}

#[test]
fn link_printer_failure_log_redacts_access_code() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let access_code = "SECRET-LINK-CODE";
    let gateway = LinkGateway::failure(access_code);
    let (sender, _receiver) = mpsc::channel(2);

    let (logs, ()) = crate::test_tracing::capture_logs(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                handle_command_with_gateway(
                    &config,
                    &gateway,
                    &sender,
                    link_printer_command(command_id, access_code),
                )
                .await
            })
            .unwrap();
    });

    let captured = logs.contents();
    assert!(captured.contains("runtime printer link failed"));
    assert!(captured.contains("[REDACTED_ACCESS_CODE]"));
    assert!(!captured.contains(access_code));
}

#[test]
fn link_failure_code_maps_bad_credentials_to_invalid_access_code() {
    for code in [
        rumqttc::ConnectReturnCode::BadUserNamePassword,
        rumqttc::ConnectReturnCode::NotAuthorized,
    ] {
        let err = anyhow::Error::new(rumqttc::ConnectionError::ConnectionRefused(code))
            .context("poll rumqttc event loop")
            .context("wait for MQTT report");
        assert_eq!(link::link_failure_code(&err), "invalid_access_code");
    }
}

#[test]
fn link_failure_code_maps_transport_errors_to_printer_unreachable() {
    let io = anyhow::Error::new(rumqttc::ConnectionError::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        "connection refused",
    )))
    .context("poll rumqttc event loop");
    assert_eq!(link::link_failure_code(&io), "printer_unreachable");

    let refused = anyhow::Error::new(rumqttc::ConnectionError::ConnectionRefused(
        rumqttc::ConnectReturnCode::ServiceUnavailable,
    ));
    assert_eq!(link::link_failure_code(&refused), "printer_unreachable");
}

#[test]
fn link_failure_code_maps_report_idle_timeout_to_printer_unreachable() {
    let err = crate::machine::mqtt::mqtt_report_idle_timeout(std::time::Duration::from_secs(10))
        .context("wait for MQTT report");
    assert_eq!(link::link_failure_code(&err), "printer_unreachable");
}

#[test]
fn link_failure_code_defaults_to_link_failed() {
    let err = anyhow::anyhow!("unexpected printer state");
    assert_eq!(link::link_failure_code(&err), "link_failed");
}
