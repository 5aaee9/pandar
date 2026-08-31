use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_operation_json_from_gcode,
};
use serde::Deserialize;

pub(crate) fn body(result: PluginHttpResult) -> String {
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

pub(crate) fn operation_json(message: &[u8]) -> PluginHttpResult {
    unsafe { pandar_plugin_operation_json_from_gcode(message.as_ptr(), message.len()) }
}

pub(crate) fn assert_operation_body_eq(result: PluginHttpResult, expected: TestOperation) {
    let actual: TestOperation = serde_json::from_str(&body(result)).unwrap();
    assert_eq!(actual, expected);
}

pub(crate) fn assert_operation_json_eq(result: PluginHttpResult, expected: serde_json::Value) {
    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body(result)).unwrap(),
        expected
    );
}

pub(crate) fn assert_stable_unsupported(result: PluginHttpResult, case: &str) {
    assert_ne!(result.status, 0, "accepted unsupported case: {case}");
    assert_eq!(result.http_code, 400, "case: {case}");
    assert_eq!(
        body(result),
        r#"{"error":"unsupported_printer_operation"}"#,
        "case: {case}"
    );
}

pub(crate) fn studio_print_message(print: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"print": print})).unwrap()
}

pub(crate) fn studio_gcode_line_message(gcode: &str) -> Vec<u8> {
    studio_print_message(serde_json::json!({
        "command": "gcode_line",
        "param": gcode,
        "sequence_id": "42"
    }))
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum TestOperation {
    Home {
        axes: Vec<String>,
    },
    MoveAxes {
        movements: Vec<TestAxisMovement>,
        feedrate_mm_per_min: u32,
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
    SetChamberLight {
        light_on: bool,
    },
    Pause,
    Resume,
    Stop,
    SetPrintSpeed {
        speed_mode: u8,
    },
    SelectExtruder {
        extruder_id: u8,
    },
    AmsRereadRfid {
        ams_id: u8,
        slot_id: u8,
    },
    AmsLoadFilament {
        ams_id: u8,
        slot_id: u8,
        global_tray_id: u16,
        extruder_id: Option<u8>,
    },
    AmsUnloadFilament {
        ams_id: u8,
        slot_id: u8,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
pub(crate) struct TestAxisMovement {
    pub(crate) axis: String,
    pub(crate) delta_mm: f64,
}
