use std::{
    io::{ErrorKind, Write},
    net::{Shutdown, TcpStream},
};

use serde_json::{Value, json};

use crate::support::request_body;

pub(super) fn login_response() -> String {
    json!({"token":"probe-token","profile":{
        "token":"probe-token","user_id":"probe-user","user_name":"Probe User",
        "tenant_id":"tenant-1","tenant_name":"Tenant"
    }})
    .to_string()
}

pub(super) fn printer_list_with_version(session_id: &str, printer_version: &str) -> String {
    json!({"message":"success","devices":[{
        "dev_id":"studio-serial-1","dev_name":"Probe Printer","name":"Probe Printer",
        "dev_model_name":"N6","model":"N6",
        "dev_online":true,"online":true,"task_status":"IDLE","state":"IDLE","gcode_state":"IDLE",
        "mc_percent":0,"mc_remaining_time":0,"layer_num":0,"total_layer_num":0,"task_id":null,
        "print_error":null,"job_id":null,"subtask_id":null,"gcode_file":null,"subtask_name":null,
        "hms":[],"pandar_printer_id":"printer-1","nozzle_temperatures":[],"active_nozzle":null,
        "bed_temperature_celsius":null,"bed_target_temperature_celsius":null,
        "chamber_temperature_celsius":null,"chamber_light_on":null,"materials":null,
        "firmware":{"session_id":session_id,"generation":5,"module_revision":8,"status_revision":9,
            "modules":modules_with_printer_version(printer_version),"upgrade_state":{"status":"UPGRADING","progress":"37"},"cfg":"101"}
    }]})
    .to_string()
}

fn modules() -> Value {
    modules_with_printer_version("01.02.03.04")
}

pub(super) fn refresh_response() -> String {
    json!({
        "command_id":"00000000-0000-0000-0000-000000000010",
        "modules":modules(),
        "module_revision":10
    })
    .to_string()
}

fn modules_with_printer_version(printer_version: &str) -> Value {
    json!([
        {"name":"ota","sw_ver":printer_version,"sw_new_ver":"01.02.04.00","new_ver":"01.02.05.00","visible":true,"product_name":"Main","sn":"SERIAL","hw_ver":"AP05","flag":5},
        {"name":"ams/0","sw_ver":"02.00.00.00","sw_new_ver":"02.00.01.00","new_ver":"02.00.02.00","visible":false,"product_name":"AMS","sn":"AMS0","hw_ver":"AMS01","flag":1},
        {"name":"n3f/0","sw_ver":"02.01.00.00","sw_new_ver":"02.01.01.00","new_ver":"02.01.02.00","visible":true,"product_name":"AMS 2 Pro","sn":"N3F0","hw_ver":"N3F01","flag":2},
        {"name":"n3s/0","sw_ver":"03.00.00.00","sw_new_ver":"03.00.01.00","new_ver":"03.00.02.00","visible":false,"product_name":"AMS-HT","sn":"N3S0","hw_ver":"N3S01","flag":3},
        {"name":"future/9","sw_ver":"09.09.09.09","sw_new_ver":"09.09.10.00","new_ver":"09.09.11.00","visible":true,"product_name":"Future","sn":"F9","hw_ver":"F09","flag":9}
    ])
}

pub(super) fn catalog_response(populated: bool) -> String {
    let catalog = if populated {
        json!([
            {"target":"printer","version":"01.02.04.00","url":"main.bin","description":"Main release"},
            {"target":"ams","version":"03.01.00.00","url":"ams.bin","description":"AMS release"}
        ])
    } else {
        json!([])
    };
    json!({"firmware":{"module_revision":8,"status_revision":9},"catalog":catalog}).to_string()
}

pub(super) fn json_body(request: &str) -> Value {
    serde_json::from_str(request_body(request)).expect("typed firmware request JSON")
}

pub(super) fn string_field(value: &Value, field: &str) -> String {
    value[field].as_str().expect("string field").to_owned()
}

pub(super) fn is_delayed_refresh(request: &str) -> bool {
    request.lines().next().unwrap_or_default()
        == "POST /api/v1/plugin/printers/printer-1/firmware/refresh HTTP/1.1"
        && request.contains(r#""sequence_id":"c-lock-overlap-version""#)
}

pub(super) fn respond(stream: &mut TcpStream, status: &str, body: String) {
    let response = format!(
        "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    match stream.write_all(response.as_bytes()) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::BrokenPipe | ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset
            ) =>
        {
            return;
        }
        Err(error) => panic!("firmware mock response failed: {error}"),
    }
    stream.flush().unwrap();
    let _ = stream.shutdown(Shutdown::Write);
}
