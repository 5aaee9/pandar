use std::{
    net::TcpListener,
    time::{Duration, Instant},
};

use pandar_network_plugin::firmware::{FirmwarePluginSession, FirmwareSendOutcome, FirmwareTunnel};

use crate::support::request_body;

use super::support::{
    Action, PREPARED, URL, acknowledged, acknowledged_with_phase, assert_redacted, mock_hub,
    start_message,
};

#[test]
fn firmware_http_prepare_is_url_free_execute_is_exact_and_never_retried() {
    let (hub, server) = mock_hub(vec![
        Action::json("200 OK", PREPARED),
        Action::json("200 OK", acknowledged("success")),
    ]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let mut diagnostics = Vec::new();

    let result = session.send_with_diagnostics(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        1,
        &mut diagnostics,
    );
    let requests = server.join().unwrap();

    assert_eq!(result.outcome, FirmwareSendOutcome::Acknowledged);
    assert!(result.callback_token.is_some());
    assert_eq!(requests.len(), 2, "execute must be attempted exactly once");
    let prepare = request_body(&requests[0]);
    assert!(requests[0].starts_with("POST /api/v1/plugin/printers/printer-1/firmware/prepare"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(prepare).unwrap(),
        serde_json::json!({
            "command":"start","sequence_id":"9001","src_id":1,
            "module":"ota","version":"01.02.03.04"
        })
    );
    assert!(!prepare.contains("SENTINEL"));
    assert!(!prepare.contains("user:secret"));

    let execute = request_body(&requests[1]);
    assert!(requests[1].starts_with("POST /api/v1/plugin/printers/printer-1/firmware/execute"));
    let execute_json: serde_json::Value = serde_json::from_str(execute).unwrap();
    assert_eq!(execute_json["prepared_token"], "prepared-1");
    assert_eq!(execute_json["command"]["url"], URL);
    assert_eq!(execute.matches(URL).count(), 1);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.matches(URL).count())
            .sum::<usize>(),
        1
    );
    assert_redacted(&diagnostics);
}

#[test]
fn firmware_http_failure_before_prepare_completes_is_safe_abi_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hub = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let session = FirmwarePluginSession::new(hub, "token".into(), 1);
    let mut diagnostics = Vec::new();

    let result = session.send_with_diagnostics(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        1,
        &mut diagnostics,
    );

    assert_eq!(result.outcome, FirmwareSendOutcome::PrePublishFailure);
    assert!(result.callback_token.is_none());
    assert_redacted(&diagnostics);
}

#[test]
fn firmware_http_ambiguity_after_prepared_token_is_success_without_retry_or_callback() {
    for actions in [
        vec![Action::json("200 OK", PREPARED)],
        vec![Action::json("200 OK", PREPARED), Action::Drop],
    ] {
        let (hub, server) = mock_hub(actions);
        let session = FirmwarePluginSession::new(hub, "token".into(), 2);
        let mut diagnostics = Vec::new();

        let result = session.send_with_diagnostics(
            "SERIAL",
            "printer-1",
            &start_message(),
            FirmwareTunnel::Local,
            2,
            &mut diagnostics,
        );
        let requests = server.join().unwrap();

        assert_eq!(result.outcome, FirmwareSendOutcome::OutcomeUnknown);
        assert!(result.callback_token.is_none());
        assert!(requests.len() <= 2, "execute was retried: {requests:#?}");
        assert_redacted(&diagnostics);
    }
}

#[test]
fn firmware_http_only_typed_execute_pre_publish_failure_is_abi_failure() {
    let (hub, server) = mock_hub(vec![
        Action::json("200 OK", PREPARED),
        Action::json(
            "409 Conflict",
            r#"{"error":"firmware_pre_publish_failure","phase":"pre_publish_failure"}"#,
        ),
    ]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 3);
    let mut diagnostics = Vec::new();

    let result = session.send_with_diagnostics(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        3,
        &mut diagnostics,
    );
    let requests = server.join().unwrap();

    assert_eq!(requests.len(), 2);
    assert_eq!(result.outcome, FirmwareSendOutcome::PrePublishFailure);
    assert!(result.callback_token.is_none());
    assert_redacted(&diagnostics);
}

#[test]
fn firmware_http_5xx_or_malformed_execute_response_is_outcome_unknown() {
    for execute in [
        Action::json(
            "500 Internal Server Error",
            r#"{"error":"internal_server_error"}"#,
        ),
        Action::json("200 OK", "not-json"),
    ] {
        let (hub, server) = mock_hub(vec![Action::json("200 OK", PREPARED), execute]);
        let session = FirmwarePluginSession::new(hub, "token".into(), 3);
        let mut diagnostics = Vec::new();
        let result = session.send_with_diagnostics(
            "SERIAL",
            "printer-1",
            &start_message(),
            FirmwareTunnel::Cloud,
            3,
            &mut diagnostics,
        );
        assert_eq!(server.join().unwrap().len(), 2);
        assert_eq!(result.outcome, FirmwareSendOutcome::OutcomeUnknown);
        assert!(result.callback_token.is_none());
        assert_redacted(&diagnostics);
    }
}

#[test]
fn firmware_http_acknowledgement_and_rejection_queue_exact_origin_callback() {
    for (phase, result_value, expected) in [
        ("acknowledged", "success", FirmwareSendOutcome::Acknowledged),
        ("rejected", "fail", FirmwareSendOutcome::Rejected),
    ] {
        let (hub, server) = mock_hub(vec![
            Action::json("200 OK", PREPARED),
            Action::json("200 OK", acknowledged_with_phase(phase, result_value)),
        ]);
        let session = FirmwarePluginSession::new(hub, "token".into(), 4);
        let response = session.send(
            "SERIAL",
            "printer-1",
            &start_message(),
            FirmwareTunnel::Cloud,
            4,
        );
        assert_eq!(server.join().unwrap().len(), 2);
        assert_eq!(response.outcome, expected);
        let token = response.callback_token.expect("real acknowledgement token");
        let handoff = Instant::now();
        assert!(session.return_handoff_at(token, 44, 0, 0, handoff));
        let callback = session
            .take_ready_callback_at(handoff + Duration::from_millis(1_100))
            .unwrap();
        assert_eq!(callback.dev_id, "SERIAL");
        assert_eq!(callback.tunnel, FirmwareTunnel::Cloud);
        let body: serde_json::Value = serde_json::from_str(&callback.message).unwrap();
        assert_eq!(body["upgrade"]["command"], "start");
        assert_eq!(body["upgrade"]["sequence_id"], "9001");
        assert_eq!(body["upgrade"]["result"], result_value);
    }
}

#[test]
fn firmware_http_published_without_ack_is_success_without_callback() {
    let (hub, server) = mock_hub(vec![
        Action::json("200 OK", PREPARED),
        Action::json(
            "200 OK",
            r#"{"command_id":"00000000-0000-0000-0000-000000000001","phase":"outcome_unknown","outcome":{"outcome":"published_without_acknowledgement"}}"#,
        ),
    ]);
    let session = FirmwarePluginSession::new(hub, "token".into(), 5);
    let result = session.send(
        "SERIAL",
        "printer-1",
        &start_message(),
        FirmwareTunnel::Cloud,
        5,
    );
    assert_eq!(server.join().unwrap().len(), 2);
    assert_eq!(
        result.outcome,
        FirmwareSendOutcome::PublishedWithoutAcknowledgement
    );
    assert!(result.callback_token.is_none());
}
