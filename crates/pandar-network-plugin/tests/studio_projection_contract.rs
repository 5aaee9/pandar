#![cfg(any(unix, windows))]

#[path = "studio_projection_contract/compiler.rs"]
mod compiler;
#[path = "studio_projection_contract/pinned.rs"]
mod pinned;

use pandar_network_plugin::{PluginHttpResult, pandar_plugin_free_with_capacity};

unsafe extern "C" {
    fn pandar_plugin_printer_telemetry_json(
        printer_ptr: *const u8,
        printer_len: usize,
    ) -> PluginHttpResult;
}

fn telemetry(printer: &str) -> serde_json::Value {
    let result = unsafe { pandar_plugin_printer_telemetry_json(printer.as_ptr(), printer.len()) };
    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    let bytes = unsafe { std::slice::from_raw_parts(result.body_ptr, result.body_len) };
    let fragment = String::from_utf8(bytes.to_vec()).unwrap();
    pandar_plugin_free_with_capacity(result.body_ptr.cast(), result.body_len, result.body_cap);
    serde_json::from_str(&format!("{{{fragment}}}")).unwrap()
}

fn projected(raw: &str) -> (serde_json::Value, serde_json::Value) {
    let telemetry = telemetry(raw);
    let consumed = compiler::run(&telemetry.to_string());
    (telemetry, consumed)
}

#[test]
fn compiled_pinned_studio_consumers_reject_unavailable_projection_capabilities() {
    assert_eq!(
        pinned::STUDIO_COMMIT,
        "ba049f6a2e08c3b6033660bb84da80c08722974b"
    );
    let (telemetry, consumed) = projected(
        r#"{"fun":"FFFFFFFFFFFFFFFF","chamber_temperature_celsius":"32","materials":{"cfg":"FFFFFFFFFFFFFFFF","aux":"00002000","stat":"0","ams_units":[]}}"#,
    );

    assert_eq!(telemetry["sdcard"], false);
    assert_eq!(telemetry["support_chamber"], false);
    assert_eq!(telemetry["chamber_temper"], 32);
    assert_eq!(telemetry["ctt"], 0);
    assert_eq!(consumed["gate"], true);
    assert_eq!(consumed["sdcard_state"], 2);
    assert_eq!(consumed["camera_hidden"], true);
    assert_eq!(consumed["chamber"], false);
    assert_eq!(consumed["chamber_display"], false);
    assert_eq!(consumed["snapshot_detection"], false);
    assert_eq!(consumed["unsupported_fun_hidden"], true);
    assert_eq!(consumed["ext_change_assist_supported"], false);
    assert_eq!(consumed["axis_homing"], true);
    assert_eq!(consumed["axis_control"], true);
}

#[test]
fn compiled_pinned_studio_consumers_accept_known_supported_projection_cases() {
    let (telemetry, consumed) = projected(
        r#"{"fun":"4100000000","chamber_temperature_celsius":"32","chamber_target_temperature_celsius":"45","materials":{"cfg":"1","aux":"00001000","stat":"0","ams_units":[]}}"#,
    );

    assert_eq!(telemetry["sdcard"], true);
    assert_eq!(telemetry["support_chamber"], true);
    assert_eq!(telemetry["chamber_temper"], 32);
    assert_eq!(telemetry["ctt"], 45);
    assert_eq!(consumed["gate"], true);
    assert_eq!(consumed["sdcard_state"], 1);
    assert_eq!(consumed["camera_hidden"], true);
    assert_eq!(consumed["chamber"], true);
    assert_eq!(consumed["chamber_display"], true);
    assert_eq!(consumed["axis_homing"], true);
    assert_eq!(consumed["axis_control"], true);
}

#[test]
fn pinned_wtm_capability_is_visible_upstream_and_hidden_by_the_plugin() {
    let raw = r#"{"fun":"1000000000000000","cfg":"","aux":"0","stat":"0"}"#;
    let upstream = compiler::run(raw);
    assert_eq!(upstream["nozzle_rack_supported"], true);

    let (telemetry, projected) = projected(raw);
    assert_eq!(telemetry["fun"], "0");
    assert_eq!(projected["nozzle_rack_supported"], false);
}

#[test]
fn pinned_external_change_assist_is_visible_upstream_and_hidden_by_the_plugin() {
    let raw = r#"{"fun":"1000000000000","cfg":"","aux":"0","stat":"0"}"#;
    let upstream = compiler::run(raw);
    assert_eq!(upstream["ext_change_assist_supported"], true);

    let (telemetry, projected) = projected(raw);
    assert_eq!(telemetry["fun"], "0");
    assert_eq!(projected["ext_change_assist_supported"], false);
}

#[test]
fn compiled_pinned_studio_consumer_observes_cloud_connection_type() {
    let (telemetry, consumed) =
        projected(r#"{"materials":{"cfg":"1","aux":"00001000","stat":"0","ams_units":[]}}"#);

    assert_eq!(telemetry["device"]["connection_type"], "cloud");
    assert_eq!(consumed["connection_type"], "cloud");
}
