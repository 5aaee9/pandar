use serde::{Deserialize, Serialize};

use super::{TOKEN, body, one_shot_server, submit_printer_operation, support::request_body};

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum TestOperation {
    Home {
        axes: Vec<String>,
    },
    SelectExtruder {
        extruder_id: u8,
    },
    SetHotendTemperature {
        temperature_celsius: u16,
        wait: bool,
        extruder_id: Option<u8>,
    },
    SetBedTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    SetChamberTemperature {
        temperature_celsius: u16,
        wait: bool,
    },
    ToggleLight,
    SetChamberLight {
        light_on: bool,
    },
    AmsRereadRfid {
        ams_id: u8,
        slot_id: u8,
    },
    AmsLoadFilament {
        ams_id: u8,
        slot_id: u8,
        global_tray_id: u16,
        external_id: String,
        extruder_id: u8,
    },
    AmsUnloadFilament {
        ams_id: u8,
        slot_id: u8,
    },
    HandlePrintError {
        error_action: TestPrintErrorAction,
        print_error: u32,
        printer_job_id: String,
        sequence_id: u64,
    },
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TestPrintErrorAction {
    Resume,
    Ignore,
    Stop,
}

fn assert_printer_operation_request(request: &str) {
    assert_eq!(
        serde_json::from_str::<TestOperation>(request_body(request)).unwrap(),
        TestOperation::Home {
            axes: vec!["x".to_owned()]
        }
    );
    assert!(
        !request_body(request).contains("G28"),
        "operation request leaked raw G-code: {request}"
    );
}

fn assert_native_print_error_request(request: &str) {
    assert_eq!(
        serde_json::from_str::<TestOperation>(request_body(request)).unwrap(),
        TestOperation::HandlePrintError {
            error_action: TestPrintErrorAction::Resume,
            print_error: 83_918_929,
            printer_job_id: "".to_owned(),
            sequence_id: 20_042,
        }
    );
}

#[test]
fn submit_printer_operation_posts_semantic_body_to_plugin_endpoint() {
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/printers/printer/operations",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 202 Accepted",
        r#"{"command_id":"cmd","status":"queued"}"#,
        Some(assert_printer_operation_request),
    );
    let operation_body = serde_json::to_vec(&TestOperation::Home {
        axes: vec!["x".to_owned()],
    })
    .unwrap();
    let result = submit_printer_operation(hub_url.as_bytes(), TOKEN, operation_body.as_slice());

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 202);
    assert_eq!(body(result), r#"{"command_id":"cmd","status":"queued"}"#);
}

#[test]
fn submit_printer_operation_posts_exact_native_print_error_body() {
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/printers/printer/operations",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 202 Accepted",
        r#"{"command_id":"cmd","status":"sent"}"#,
        Some(assert_native_print_error_request),
    );
    let operation_body = serde_json::to_vec(&TestOperation::HandlePrintError {
        error_action: TestPrintErrorAction::Resume,
        print_error: 83_918_929,
        printer_job_id: "".to_owned(),
        sequence_id: 20_042,
    })
    .unwrap();
    let result = submit_printer_operation(hub_url.as_bytes(), TOKEN, operation_body.as_slice());

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 202);
    assert_eq!(body(result), r#"{"command_id":"cmd","status":"sent"}"#);
}

#[test]
fn submit_printer_operation_rejects_invalid_native_print_error_bodies() {
    for operation in [
        br#"{"action":"handle_print_error","error_action":"resume","print_error":0,"printer_job_id":"","sequence_id":20042}"#.as_slice(),
        br#"{"action":"handle_print_error","error_action":"resume","print_error":2147483648,"printer_job_id":"","sequence_id":20042}"#.as_slice(),
        br#"{"action":"handle_print_error","error_action":"unknown","print_error":83918929,"printer_job_id":"","sequence_id":20042}"#.as_slice(),
        br#"{"action":"handle_print_error","print_error":83918929,"printer_job_id":"","sequence_id":20042}"#.as_slice(),
        br#"{"action":"handle_print_error","error_action":"resume","print_error":83918929,"printer_job_id":"","sequence_id":20042,"extra":true}"#.as_slice(),
    ] {
        let result = submit_printer_operation(b"http://127.0.0.1:9", TOKEN, operation);

        assert_ne!(result.status, 0);
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"invalid_printer_operation"}"#);
    }
}

#[test]
fn submit_printer_operation_accepts_latest_agent_operation_bodies() {
    for operation in [
        TestOperation::SelectExtruder { extruder_id: 1 },
        TestOperation::SetHotendTemperature {
            temperature_celsius: 210,
            wait: false,
            extruder_id: Some(1),
        },
        TestOperation::SetBedTemperature {
            temperature_celsius: 65,
            wait: true,
        },
        TestOperation::SetChamberTemperature {
            temperature_celsius: 50,
            wait: false,
        },
        TestOperation::ToggleLight,
        TestOperation::SetChamberLight { light_on: true },
        TestOperation::AmsRereadRfid {
            ams_id: 1,
            slot_id: 2,
        },
        TestOperation::AmsLoadFilament {
            ams_id: 1,
            slot_id: 2,
            global_tray_id: 6,
            external_id: "slot-2".to_owned(),
            extruder_id: 0,
        },
        TestOperation::AmsUnloadFilament {
            ams_id: 1,
            slot_id: 2,
        },
    ] {
        let hub_url = one_shot_server(
            "POST",
            "/api/v1/plugin/printers/printer/operations",
            Some("pandar_plugin_test_token"),
            "HTTP/1.1 202 Accepted",
            r#"{"command_id":"cmd","status":"queued"}"#,
            None,
        );
        let operation_body = serde_json::to_vec(&operation).unwrap();
        let result = submit_printer_operation(hub_url.as_bytes(), TOKEN, operation_body.as_slice());

        assert_eq!(result.status, 0);
        assert_eq!(result.http_code, 202);
        assert_eq!(body(result), r#"{"command_id":"cmd","status":"queued"}"#);
    }
}

#[test]
fn submit_printer_operation_rejects_invalid_json_before_network() {
    let result = submit_printer_operation(b"http://127.0.0.1:9", TOKEN, b"not-json");

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_printer_operation"}"#);
}

#[test]
fn submit_printer_operation_rejects_unknown_action_before_network() {
    let result = submit_printer_operation(b"http://127.0.0.1:9", TOKEN, br#"{"action":"dance"}"#);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_printer_operation"}"#);
}

#[test]
fn submit_printer_operation_rejects_invalid_latest_agent_operation_values_before_network() {
    for operation in [
        br#"{"action":"select_extruder","extruder_id":2}"#.as_slice(),
        br#"{"action":"set_hotend_temperature","temperature_celsius":301}"#.as_slice(),
        br#"{"action":"set_hotend_temperature","temperature_celsius":210,"extruder_id":2}"#
            .as_slice(),
        br#"{"action":"set_bed_temperature","temperature_celsius":121}"#.as_slice(),
        br#"{"action":"set_chamber_temperature","temperature_celsius":71}"#.as_slice(),
        br#"{"action":"ams_reread_rfid","ams_id":256,"slot_id":1}"#.as_slice(),
        br#"{"action":"ams_load_filament","ams_id":1,"slot_id":256}"#.as_slice(),
        br#"{"action":"ams_unload_filament","ams_id":1,"slot_id":2,"extruder_id":2}"#.as_slice(),
        br#"{"action":"set_bed_temperature","temperature_celsius":60,"ams_id":1}"#.as_slice(),
    ] {
        let result = submit_printer_operation(b"http://127.0.0.1:9", TOKEN, operation);

        assert_ne!(result.status, 0);
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"invalid_printer_operation"}"#);
    }
}

#[test]
fn submit_printer_operation_preserves_stable_operation_errors() {
    let hub_url = one_shot_server(
        "POST",
        "/api/v1/plugin/printers/printer/operations",
        Some("pandar_plugin_test_token"),
        "HTTP/1.1 400 Bad Request",
        r#"{"error":"printer_operation_unavailable"}"#,
        Some(assert_printer_operation_request),
    );
    let operation_body = serde_json::to_vec(&TestOperation::Home {
        axes: vec!["x".to_owned()],
    })
    .unwrap();
    let result = submit_printer_operation(hub_url.as_bytes(), TOKEN, operation_body.as_slice());

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"printer_operation_unavailable"}"#);
}
