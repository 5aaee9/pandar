use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_exchange_ticket, pandar_plugin_free_with_capacity,
    pandar_plugin_submit_print,
};

fn body(result: PluginHttpResult) -> String {
    if result.body_ptr.is_null() || result.body_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    body
}

#[test]
fn exchange_ticket_rejects_empty_ticket_before_network() {
    let hub = b"http://127.0.0.1:9";
    let result = pandar_plugin_exchange_ticket(hub.as_ptr(), hub.len(), b"".as_ptr(), 0);

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"invalid_plugin_ticket"}"#);
}

#[test]
fn submit_print_rejects_missing_artifact_before_network() {
    let hub = b"http://127.0.0.1:9";
    let token = b"pandar_plugin_test";
    let printer_id = b"printer";
    let filename = b"job.3mf";
    let artifact_path = b"/path/that/does/not/exist/job.3mf";
    let result = pandar_plugin_submit_print(
        hub.as_ptr(),
        hub.len(),
        token.as_ptr(),
        token.len(),
        printer_id.as_ptr(),
        printer_id.len(),
        filename.as_ptr(),
        filename.len(),
        artifact_path.as_ptr(),
        artifact_path.len(),
        1,
        true,
        false,
        false,
        b"".as_ptr(),
        0,
        b"".as_ptr(),
        0,
    );

    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
    assert_eq!(body(result), r#"{"error":"artifact_missing"}"#);
}
