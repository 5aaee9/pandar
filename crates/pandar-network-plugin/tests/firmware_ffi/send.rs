use super::{
    abi::Session,
    support::{PREPARED, Response, acknowledged, command, mock_hub, probe_hub},
};
use pandar_network_plugin::firmware::PLUGIN_JSON_BODY_LIMIT;
use serde::Serialize;

#[test]
fn firmware_ffi_send_classifies_non_firmware_and_invalid_without_hub_contact() {
    let (hub, server) = probe_hub(Vec::new());
    let session = Session::create(&hub, "token", 1);
    let mut token = 99;

    let not_firmware = session.send(
        "SERIAL",
        "printer-1",
        r#"{"print":{"command":"push_status"}}"#,
        0,
        Some(&mut token),
    );
    assert_eq!((not_firmware.status, not_firmware.http_code), (2, 200));
    assert!(not_firmware.body.is_empty());
    assert_eq!(token, 0);

    token = 99;
    let invalid = session.send(
        "SERIAL",
        "printer-1",
        r#"{"upgrade":null}"#,
        0,
        Some(&mut token),
    );
    assert_eq!((invalid.status, invalid.http_code), (1, 400));
    assert_eq!(invalid.body, r#"{"error":"unsupported_printer_operation"}"#);
    assert_eq!(token, 0);

    session.destroy();
    assert!(server.join().unwrap().is_empty());
}

#[test]
fn firmware_ffi_send_classifies_valid_oversized_non_firmware_without_hub_contact() {
    let (hub, server) = probe_hub(Vec::new());
    let session = Session::create(&hub, "token", 1);
    let message = oversized_non_firmware_message();
    assert!(message.len() > PLUGIN_JSON_BODY_LIMIT);
    let mut token = 99;

    let result = session.send("SERIAL", "printer-1", &message, 0, Some(&mut token));

    assert_eq!((result.status, result.http_code), (2, 200));
    assert!(result.body.is_empty());
    assert_eq!(token, 0);

    let present_upgrade = format!(
        r#"{{"upgrade":{{"command":"upgrade_confirm","sequence_id":"{}","src_id":1}}}}"#,
        "x".repeat(PLUGIN_JSON_BODY_LIMIT)
    );
    token = 99;
    let invalid = session.send("SERIAL", "printer-1", &present_upgrade, 0, Some(&mut token));
    assert_eq!((invalid.status, invalid.http_code), (1, 400));
    assert_eq!(invalid.body, r#"{"error":"unsupported_printer_operation"}"#);
    assert_eq!(token, 0);

    session.destroy();
    let requests = server.join().unwrap();
    assert!(
        requests.is_empty(),
        "oversized firmware classification contacted Hub: {requests:?}"
    );
}

#[test]
fn firmware_ffi_send_requires_non_null_token_out_before_hub_contact() {
    let (hub, server) = probe_hub(vec![
        Response::json("200 OK", PREPARED),
        Response::json("200 OK", acknowledged("7")),
    ]);
    let session = Session::create(&hub, "token", 1);

    let result = session.send("SERIAL", "printer-1", &command("7"), 0, None);

    session.destroy();
    let requests = server.join().unwrap();
    assert!(
        result.status == 1
            && result.http_code == 400
            && result.body == r#"{"error":"invalid_firmware_request"}"#
            && requests.is_empty(),
        "null token_out returned status={} HTTP {} body={} and contacted Hub {} time(s): {requests:?}",
        result.status,
        result.http_code,
        result.body,
        requests.len()
    );
}

#[test]
fn firmware_ffi_acknowledged_send_handoff_returns_and_frees_callback_triples() {
    let (hub, server) = mock_hub(vec![
        Response::json("200 OK", PREPARED),
        Response::json("200 OK", acknowledged("8")),
    ]);
    let session = Session::create(&hub, "token", 3);
    let mut token = 0;

    let result = session.send("SERIAL", "printer-1", &command("8"), 1, Some(&mut token));
    assert_eq!((result.status, result.http_code), (0, 200));
    assert_eq!(result.body, r#"{"outcome":"acknowledged"}"#);
    assert_ne!(token, 0);
    assert_eq!(session.handoff(token, 44), 0);

    let callback = session.next_callback(1_800);
    assert_eq!(callback.status, 0);
    assert_eq!(callback.dev_id, "SERIAL");
    assert_eq!(callback.tunnel, 1);
    let body: serde_json::Value = serde_json::from_str(&callback.message).unwrap();
    assert_eq!(body["upgrade"]["command"], "upgrade_confirm");
    assert_eq!(body["upgrade"]["sequence_id"], "8");

    session.destroy();
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn firmware_ffi_generation_cancel_removes_pending_callback() {
    let (hub, server) = mock_hub(vec![
        Response::json("200 OK", PREPARED),
        Response::json("200 OK", acknowledged("9")),
    ]);
    let session = Session::create(&hub, "token", 5);
    let mut token = 0;
    let result = session.send("SERIAL", "printer-1", &command("9"), 0, Some(&mut token));
    assert_eq!(result.status, 0);
    assert_ne!(token, 0);

    session.cancel_generation(5);
    assert_eq!(session.handoff(token, 45), 1);
    let callback = session.next_callback(0);
    assert_eq!(callback.status, 1);
    assert!(callback.dev_id.is_empty());
    assert!(callback.message.is_empty());

    session.destroy();
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn firmware_ffi_generation_cancel_blocks_future_same_generation_send() {
    let (hub, server) = probe_hub(Vec::new());
    let session = Session::create(&hub, "token", 5);
    session.cancel_generation(5);
    let mut token = 99;

    let result = session.send("SERIAL", "printer-1", &command("10"), 0, Some(&mut token));

    assert_eq!((result.status, result.http_code), (1, 400));
    assert_eq!(result.body, r#"{"outcome":"pre_publish_failure"}"#);
    assert_eq!(token, 0);
    session.destroy();
    assert!(
        server.join().unwrap().is_empty(),
        "cancelled generation contacted Hub"
    );
}

#[derive(Serialize)]
struct NonFirmwareMessage {
    print: NonFirmwareStatus,
    padding: String,
}

#[derive(Serialize)]
struct NonFirmwareStatus {
    command: &'static str,
}

fn oversized_non_firmware_message() -> String {
    serde_json::to_string(&NonFirmwareMessage {
        print: NonFirmwareStatus {
            command: "push_status",
        },
        padding: "x".repeat(PLUGIN_JSON_BODY_LIMIT),
    })
    .unwrap()
}
