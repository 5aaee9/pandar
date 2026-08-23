use std::{
    collections::BTreeMap,
    net::TcpListener,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use serde::{Deserialize, Serialize};

use super::{
    responses::PRINTERS_RESPONSE,
    transport::{assert_request_with_token, write_response},
};

pub(super) fn serve_request_admission(
    listener: &TcpListener,
    stop: &AtomicBool,
    deadline: Instant,
) {
    let mut printers: PrinterList = serde_json::from_str(PRINTERS_RESPONSE).unwrap();
    printers.devices[0].pandar_printer_id = "00000000-0000-0000-0000-000000000123".to_owned();
    let printers = serde_json::to_string(&printers).unwrap();
    let mut stream_upgrades = 0;
    let mut operations = 0;
    let mut catalogs = 0;
    let mut prepares = 0;
    let mut executes = 0;
    let mut refreshes = 0;
    while !stop.load(Ordering::Acquire) {
        let (mut stream, request) = match super::next_incoming(listener, stop, deadline) {
            super::Incoming::Stream(upgrade) => {
                assert!(
                    upgrade
                        .request
                        .contains("/printer-events?projection=studio&version=1")
                        && upgrade
                            .request
                            .contains("authorization: Bearer probe-token")
                );
                stream_upgrades += 1;
                let frames = upgrade.serve();
                for frame in super::responses::snapshot_frames(&printers) {
                    frames
                        .send(frame)
                        .expect("serve request-admission snapshot");
                }
                continue;
            }
            super::Incoming::Http(stream, request) => (stream, request),
        };
        let line = request.lines().next().unwrap_or_default();
        match line {
            "POST /api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/operations HTTP/1.1"
                if operations == 0 =>
            {
                assert_request_with_token(
                    &request,
                    "POST",
                    "/api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/operations",
                    Some("probe-token"),
                );
                operations += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 202 Accepted",
                    r#"{"command_id":"00000000-0000-0000-0000-000000000010","status":"sent"}"#,
                );
            }
            "GET /api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware HTTP/1.1"
                if catalogs == 0 =>
            {
                assert_request_with_token(
                    &request,
                    "GET",
                    "/api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware",
                    Some("probe-token"),
                );
                catalogs += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"firmware":{"module_revision":1,"status_revision":1},"catalog":[]}"#,
                );
            }
            "POST /api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware/prepare HTTP/1.1"
                if prepares == 0 =>
            {
                assert_request_with_token(
                    &request,
                    "POST",
                    "/api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware/prepare",
                    Some("probe-token"),
                );
                prepares += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"command_id":"00000000-0000-0000-0000-000000000011","prepared_token":"prepared"}"#,
                );
            }
            "POST /api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware/execute HTTP/1.1"
                if executes == 0 =>
            {
                assert_request_with_token(
                    &request,
                    "POST",
                    "/api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware/execute",
                    Some("probe-token"),
                );
                executes += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"command_id":"00000000-0000-0000-0000-000000000011","phase":"outcome_unknown","outcome":{"outcome":"published_without_acknowledgement"}}"#,
                );
            }
            "POST /api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware/refresh HTTP/1.1"
                if refreshes == 0 =>
            {
                assert_request_with_token(
                    &request,
                    "POST",
                    "/api/v1/plugin/printers/00000000-0000-0000-0000-000000000123/firmware/refresh",
                    Some("probe-token"),
                );
                refreshes += 1;
                write_response(
                    &mut stream,
                    "HTTP/1.1 200 OK",
                    r#"{"command_id":"00000000-0000-0000-0000-000000000012","modules":[{"name":"ota","sw_ver":"01.02.03.04"}],"module_revision":1}"#,
                );
            }
            _ => panic!("request admission performed unexpected Hub I/O: {request}"),
        }
    }
    assert!(stream_upgrades >= 1, "missing request admission stream");
    assert_eq!(operations, 1, "authorized operation count changed");
    assert_eq!(catalogs, 1, "authorized firmware catalog count changed");
    assert_eq!(prepares, 1, "authorized firmware prepare count changed");
    assert_eq!(executes, 1, "authorized firmware execute count changed");
    assert_eq!(refreshes, 1, "authorized firmware refresh count changed");
}

#[derive(Deserialize, Serialize)]
struct PrinterList {
    devices: Vec<PrinterIdentity>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
struct PrinterIdentity {
    pandar_printer_id: String,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}
