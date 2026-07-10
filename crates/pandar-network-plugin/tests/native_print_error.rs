use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_operation_json_from_gcode,
};
use serde_json::{Value, json};

const STATUS_OPERATION: i32 = 0;
const STATUS_UNSUPPORTED: i32 = 1;
const STATUS_INVALID_NATIVE: i32 = 2;
const STABLE_UNSUPPORTED_BODY: &str = r#"{"error":"unsupported_printer_operation"}"#;

struct ParsedOperation {
    status: i32,
    http_code: u32,
    body: String,
}

fn parse(message: &[u8]) -> ParsedOperation {
    let result = pandar_plugin_operation_json_from_gcode(message.as_ptr(), message.len());
    let status = result.status;
    let http_code = result.http_code;
    let body = take_body(result);
    ParsedOperation {
        status,
        http_code,
        body,
    }
}

fn take_body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

fn native_candidate(command: &str) -> Value {
    json!({
        "print": {
            "command": command,
            "err": "83918929",
            "job_id": "",
            "param": "reserve",
            "sequence_id": "20042"
        }
    })
}

fn parse_value(value: &Value) -> ParsedOperation {
    parse(&serde_json::to_vec(value).unwrap())
}

fn assert_status(value: &Value, expected_status: i32) {
    let result = parse_value(value);
    assert_eq!(result.status, expected_status, "input: {value}");
    assert_eq!(result.http_code, 400, "input: {value}");
    assert_eq!(result.body, STABLE_UNSUPPORTED_BODY, "input: {value}");
}

#[test]
fn valid_native_actions_serialize_the_exact_typed_rest_body() {
    for (command, expected) in [
        (
            "resume",
            r#"{"action":"handle_print_error","error_action":"resume","print_error":83918929,"printer_job_id":"","sequence_id":20042}"#,
        ),
        (
            "ignore",
            r#"{"action":"handle_print_error","error_action":"ignore","print_error":83918929,"printer_job_id":"","sequence_id":20042}"#,
        ),
        (
            "stop",
            r#"{"action":"handle_print_error","error_action":"stop","print_error":83918929,"printer_job_id":"","sequence_id":20042}"#,
        ),
    ] {
        let result = parse_value(&native_candidate(command));
        assert_eq!(result.status, STATUS_OPERATION, "command: {command}");
        assert_eq!(result.http_code, 200, "command: {command}");
        assert_eq!(result.body, expected, "command: {command}");
    }
}

#[test]
fn ordinary_resume_and_stop_ignore_non_marker_job_and_sequence_fields() {
    let extras = [
        json!({}),
        json!({"job_id": "job-7"}),
        json!({"job_id": null}),
        json!({"job_id": {"wrong": true}}),
        json!({"sequence_id": "42"}),
        json!({"sequence_id": null}),
        json!({"sequence_id": [42]}),
        json!({"job_id": null, "sequence_id": false}),
    ];

    for command in ["resume", "stop"] {
        for param in [None, Some("")] {
            for extra in &extras {
                let mut print = extra.as_object().unwrap().clone();
                print.insert("command".to_owned(), json!(command));
                if let Some(param) = param {
                    print.insert("param".to_owned(), json!(param));
                }
                let input = json!({"print": print});
                let result = parse_value(&input);
                assert_eq!(result.status, STATUS_OPERATION, "input: {input}");
                assert_eq!(result.http_code, 200, "input: {input}");
                assert_eq!(result.body, format!(r#"{{"action":"{command}"}}"#));
            }
        }
    }
}

#[test]
fn unsupported_messages_use_status_one_and_the_stable_body() {
    for value in [
        json!({}),
        json!({"print": {}}),
        json!({"print": {"command": null}}),
        json!({"print": {"command": 7}}),
        json!({"print": {"command": {"wrong": true}}}),
        json!({"print": {"command": "unrelated"}}),
    ] {
        assert_status(&value, STATUS_UNSUPPORTED);
    }

    let malformed = parse(b"not-json-or-gcode");
    assert_eq!(malformed.status, STATUS_UNSUPPORTED);
    assert_eq!(malformed.http_code, 400);
    assert_eq!(malformed.body, STABLE_UNSUPPORTED_BODY);

    let invalid_utf8 = parse(&[0xff, 0xfe]);
    assert_eq!(invalid_utf8.status, STATUS_UNSUPPORTED);
    assert_eq!(invalid_utf8.http_code, 400);
    assert_eq!(invalid_utf8.body, STABLE_UNSUPPORTED_BODY);
}

#[test]
fn command_known_candidates_missing_each_required_field_are_invalid_native() {
    for command in ["resume", "ignore", "stop"] {
        for field in ["param", "err", "job_id", "sequence_id"] {
            let mut value = native_candidate(command);
            value["print"].as_object_mut().unwrap().remove(field);
            assert_status(&value, STATUS_INVALID_NATIVE);
        }
    }
}

#[test]
fn wrong_typed_and_null_candidate_fields_are_invalid_native() {
    for command in ["resume", "ignore", "stop"] {
        for field in ["param", "err", "job_id", "sequence_id"] {
            for wrong in [json!(17), Value::Null] {
                let mut value = native_candidate(command);
                value["print"][field] = wrong;
                assert_status(&value, STATUS_INVALID_NATIVE);
            }
        }
    }
}

#[test]
fn non_reserve_or_missing_param_never_downgrades_a_candidate() {
    for command in ["resume", "ignore", "stop"] {
        for param in [None, Some(""), Some("other"), Some("Reserve")] {
            let mut value = native_candidate(command);
            match param {
                Some(param) => value["print"]["param"] = json!(param),
                None => {
                    value["print"].as_object_mut().unwrap().remove("param");
                }
            }
            assert_status(&value, STATUS_INVALID_NATIVE);
        }
    }
}

#[test]
fn print_error_must_be_a_positive_signed_32_bit_decimal_string() {
    for err in ["0", "-1", "not-decimal", "2147483648"] {
        let mut value = native_candidate("resume");
        value["print"]["err"] = json!(err);
        assert_status(&value, STATUS_INVALID_NATIVE);
    }

    let mut maximum = native_candidate("resume");
    maximum["print"]["err"] = json!(i32::MAX.to_string());
    let result = parse_value(&maximum);
    assert_eq!(result.status, STATUS_OPERATION);
    assert_eq!(
        result.body,
        r#"{"action":"handle_print_error","error_action":"resume","print_error":2147483647,"printer_job_id":"","sequence_id":20042}"#
    );
}

#[test]
fn sequence_accepts_the_full_u64_domain_and_rejects_invalid_decimals() {
    for sequence_id in ["-1", "not-decimal", "18446744073709551616"] {
        let mut value = native_candidate("stop");
        value["print"]["sequence_id"] = json!(sequence_id);
        assert_status(&value, STATUS_INVALID_NATIVE);
    }

    for (sequence_id, expected) in [
        ("00042", "42"),
        ("18446744073709551615", "18446744073709551615"),
    ] {
        let mut value = native_candidate("stop");
        value["print"]["sequence_id"] = json!(sequence_id);
        let result = parse_value(&value);
        assert_eq!(result.status, STATUS_OPERATION, "input: {value}");
        assert_eq!(
            result.body,
            format!(
                r#"{{"action":"handle_print_error","error_action":"stop","print_error":83918929,"printer_job_id":"","sequence_id":{expected}}}"#
            )
        );
    }
}

#[test]
fn every_partial_ignore_shape_is_an_invalid_native_candidate() {
    let fields = [
        ("param", json!("reserve")),
        ("err", json!("83918929")),
        ("job_id", json!("")),
        ("sequence_id", json!("20042")),
    ];
    for mask in 0_u8..0b1111 {
        let mut print = serde_json::Map::new();
        print.insert("command".to_owned(), json!("ignore"));
        for (index, (field, value)) in fields.iter().enumerate() {
            if mask & (1 << index) != 0 {
                print.insert((*field).to_owned(), value.clone());
            }
        }
        assert_status(&json!({"print": print}), STATUS_INVALID_NATIVE);
    }
}

#[test]
fn invalid_native_candidates_never_reach_ordinary_or_raw_gcode_fallback() {
    let mut value = native_candidate("resume");
    value["print"]["err"] = json!("not-decimal");
    value["gcode"] = json!("G28 X");

    assert_status(&value, STATUS_INVALID_NATIVE);
}
