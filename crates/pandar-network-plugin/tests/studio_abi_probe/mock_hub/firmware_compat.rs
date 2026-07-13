use crate::support::request_body;

use super::transport::write_response;

pub(super) fn try_respond(stream: &mut std::net::TcpStream, request: &str) -> bool {
    let line = request.lines().next().unwrap_or_default();
    if !line.starts_with("POST /api/v1/plugin/printers/")
        || !line.ends_with("/firmware/refresh HTTP/1.1")
    {
        return false;
    }
    let request: serde_json::Value =
        serde_json::from_str(request_body(request)).expect("firmware refresh request JSON");
    let sequence_id = request["sequence_id"]
        .as_str()
        .expect("firmware refresh sequence");
    let response = serde_json::json!({
        "command_id":"00000000-0000-0000-0000-000000000099",
        "modules":[{
            "name":"ota","product_name":"N6","sw_ver":"01.02.03.04",
            "sw_new_ver":"","hw_ver":"OTA","sn":"studio-serial-1","flag":0
        }],
        "module_revision":1
    })
    .to_string();
    assert!(!sequence_id.is_empty());
    write_response(stream, "HTTP/1.1 200 OK", &response);
    true
}
