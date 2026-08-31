use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_classify_status_request, pandar_plugin_free_with_capacity,
};
use serde_json::json;

const STATUS_NOT_REQUEST: i32 = 0;
const STATUS_GET_VERSION: i32 = 1;
const STATUS_PUSH_ALL: i32 = 2;

fn classify(message: &[u8]) -> (i32, u32, String) {
    let result = unsafe { pandar_plugin_classify_status_request(message.as_ptr(), message.len()) };
    let status = result.status;
    let http_code = result.http_code;
    let body = take_body(result);
    (status, http_code, body)
}

fn take_body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    unsafe {
        pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap)
    };
    body
}

fn classify_json(value: serde_json::Value) -> (i32, u32, String) {
    classify(&serde_json::to_vec(&value).unwrap())
}

#[test]
fn exact_top_level_status_requests_return_their_sequence() {
    assert_eq!(
        classify_json(json!({"info": {"command": "get_version", "sequence_id": "30001"}})),
        (STATUS_GET_VERSION, 200, "30001".to_owned())
    );
    assert_eq!(
        classify_json(json!({
            "pushing": {
                "command": "pushall",
                "sequence_id": "30002",
                "version": 1,
                "push_target": 1
            }
        })),
        (STATUS_PUSH_ALL, 200, "30002".to_owned())
    );
}

#[test]
fn lookalikes_and_native_job_id_collisions_are_not_status_requests() {
    for value in [
        json!({"info": {"command": "not_get_version", "sequence_id": "30003"}}),
        json!({"pushing": {"command": "not_pushall", "sequence_id": "30004"}}),
        json!({
            "print": {
                "command": "resume",
                "err": "83918929",
                "job_id": "resume-get_version-pushall",
                "param": "reserve",
                "sequence_id": "20042"
            }
        }),
        json!({
            "print": {
                "command": "ignore",
                "err": "83918929",
                "job_id": "ignore-get_version-pushall",
                "param": "reserve",
                "sequence_id": "20043"
            }
        }),
        json!({
            "print": {
                "command": "stop",
                "err": "83918929",
                "job_id": "stop-get_version-pushall",
                "param": "reserve",
                "sequence_id": "20044"
            }
        }),
    ] {
        assert_eq!(
            classify_json(value),
            (STATUS_NOT_REQUEST, 200, String::new())
        );
    }
}

#[test]
fn mixed_or_extra_top_level_envelopes_are_not_status_requests() {
    for value in [
        json!({
            "info": {"command": "get_version", "sequence_id": "31001"},
            "pushing": {
                "command": "pushall",
                "sequence_id": "31002",
                "version": 1,
                "push_target": 1
            }
        }),
        json!({
            "info": {"command": "get_version", "sequence_id": "31003"},
            "print": {"command": "pause", "sequence_id": "31004"}
        }),
        json!({
            "pushing": {
                "command": "pushall",
                "sequence_id": "31005",
                "version": 1,
                "push_target": 1
            },
            "print": {"command": "pause", "sequence_id": "31006"}
        }),
        json!({
            "info": {"command": "get_version", "sequence_id": "31007"},
            "future": {"command": "future_control"}
        }),
    ] {
        assert_eq!(
            classify_json(value.clone()),
            (STATUS_NOT_REQUEST, 200, String::new()),
            "mixed or extra envelope was classified as status: {value}"
        );
    }
}
