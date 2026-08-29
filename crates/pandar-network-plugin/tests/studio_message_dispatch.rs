use pandar_network_plugin::{
    PluginStudioMessageResult, pandar_plugin_dispatch_studio_message,
    pandar_plugin_free_with_capacity,
};

const UNSUPPORTED: i32 = 0;
const FIRMWARE: i32 = 1;
const STATUS_GET_VERSION: i32 = 2;
const STATUS_PUSH_ALL: i32 = 3;
const OPERATION: i32 = 4;
const H2C_AUTO_NOZZLE_MAPPING: i32 = 5;

fn dispatch(message: &str) -> (i32, i32, i32, String) {
    let result = pandar_plugin_dispatch_studio_message(message.as_ptr(), message.len());
    result_parts(result)
}

fn result_parts(result: PluginStudioMessageResult) -> (i32, i32, i32, String) {
    let body = if result.body_ptr.is_null() || result.body_len == 0 {
        String::new()
    } else {
        String::from_utf8(
            unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) }.to_vec(),
        )
        .unwrap()
    };
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    (result.kind, result.outcome, result.abi_status, body)
}

#[test]
fn classifier_owns_firmware_status_operation_priority() {
    assert_eq!(
        dispatch(r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"7","src_id":1}}"#),
        (FIRMWARE, 0, 0, String::new())
    );
    assert_eq!(
        dispatch(r#"{"info":{"command":"get_version","sequence_id":"8"}}"#),
        (STATUS_GET_VERSION, 0, 0, "8".to_owned())
    );
    assert_eq!(
        dispatch(r#"{"pushing":{"command":"pushall","sequence_id":"9"}}"#),
        (STATUS_PUSH_ALL, 0, 0, "9".to_owned())
    );

    let (kind, outcome, abi_status, body) =
        dispatch(r#"{"print":{"command":"pause","sequence_id":"10"}}"#);
    assert_eq!((kind, outcome, abi_status), (OPERATION, 0, 0));
    assert_eq!(body, r#"{"action":"pause"}"#);
}

#[test]
fn invalid_or_ambiguous_firmware_never_falls_through() {
    let ambiguous = dispatch(
        r#"{"upgrade":{"command":"upgrade_confirm","sequence_id":"7","src_id":1},"pushing":{"command":"pushall","sequence_id":"9"}}"#,
    );
    assert_eq!(ambiguous.0, FIRMWARE);
    assert_ne!(ambiguous.1, 0);
    assert_eq!(ambiguous.2, -19);
    assert_eq!(ambiguous.3, r#"{"error":"unsupported_printer_operation"}"#);

    let invalid = dispatch(
        r#"{"upgrade":{"command":"start","sequence_id":"7","src_id":1,"url":"","module":"ota","version":"1"}}"#,
    );
    assert_eq!(invalid.0, FIRMWARE);
    assert_ne!(invalid.1, 0);
    assert_eq!(invalid.2, -19);
}

#[test]
fn h2c_auto_mapping_is_classified_and_forwarded_as_a_typed_request() {
    let (kind, outcome, abi_status, body) = dispatch(
        r#"{"print":{"command":"get_auto_nozzle_mapping","sequence_id":"42","version":1,"group_info":[{"id":0,"ext":1,"dia":0.4,"vol":"E3D High Flow"}]}}"#,
    );

    assert_eq!((kind, outcome, abi_status), (H2C_AUTO_NOZZLE_MAPPING, 0, 0));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({
            "command": "get_auto_nozzle_mapping",
            "sequence_id": "42",
            "version": 1,
            "group_info": [{
                "id": 0,
                "ext": 1,
                "dia": 0.4,
                "vol": "E3D High Flow"
            }]
        })
    );
}

#[test]
fn malformed_h2c_auto_mapping_never_falls_through_to_an_ordinary_operation() {
    let result = dispatch(
        r#"{"print":{"command":"get_auto_nozzle_mapping","sequence_id":"42","version":"1","group_info":[{"id":0,"ext":1,"dia":0.4,"vol":"E3D High Flow"}]}}"#,
    );

    assert_eq!(result.0, H2C_AUTO_NOZZLE_MAPPING);
    assert_ne!(result.1, 0);
    assert_eq!(result.2, -19);
}

#[test]
fn unsupported_message_has_one_stable_non_success_outcome() {
    assert_eq!(
        dispatch(r#"{"unknown":{"command":"noop"}}"#),
        (
            UNSUPPORTED,
            1,
            -19,
            r#"{"error":"unsupported_printer_operation"}"#.to_owned(),
        )
    );
}
