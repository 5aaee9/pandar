use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_operation_json_from_gcode,
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

fn operation_json(message: &[u8]) -> PluginHttpResult {
    pandar_plugin_operation_json_from_gcode(message.as_ptr(), message.len())
}

fn assert_json_body_eq(result: PluginHttpResult, expected: serde_json::Value) {
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body(result)).unwrap(),
        expected
    );
}

#[test]
fn gcode_parser_maps_home_and_axes_to_semantic_json() {
    let result = operation_json(b"  G28 X Z ; home selected axes\n");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_json_body_eq(
        result,
        serde_json::json!({"action":"home","axes":["x","z"]}),
    );
}

#[test]
fn gcode_parser_maps_relative_move_to_semantic_json() {
    let result = operation_json(b"G91\nG0 X10.5 Z-0.25 F3000");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_json_body_eq(
        result,
        serde_json::json!({
            "action": "move_axes",
            "movements": [
                { "axis": "x", "delta_mm": 10.5 },
                { "axis": "z", "delta_mm": -0.25 }
            ],
            "feedrate_mm_per_min": 3000,
        }),
    );
}

#[test]
fn gcode_parser_maps_hotend_temperature_to_semantic_json() {
    let result = operation_json(b"M109 S215");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_json_body_eq(
        result,
        serde_json::json!({
            "action": "set_hotend_temperature",
            "temperature_celsius": 215,
            "wait": true
        }),
    );
}

#[test]
fn gcode_parser_maps_targeted_hotend_temperature_to_semantic_json() {
    let result = operation_json(b"M104 S210 T1");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_json_body_eq(
        result,
        serde_json::json!({
            "action": "set_hotend_temperature",
            "temperature_celsius": 210,
            "wait": false,
            "extruder_id": 1,
        }),
    );
}

#[test]
fn gcode_parser_maps_bed_and_chamber_temperature_to_semantic_json() {
    for (message, expected) in [
        (
            b"M140 S60".as_slice(),
            serde_json::json!({
                "action": "set_bed_temperature",
                "temperature_celsius": 60,
                "wait": false,
            }),
        ),
        (
            b"M190 S65".as_slice(),
            serde_json::json!({
                "action": "set_bed_temperature",
                "temperature_celsius": 65,
                "wait": true,
            }),
        ),
        (
            b"M141 S45".as_slice(),
            serde_json::json!({
                "action": "set_chamber_temperature",
                "temperature_celsius": 45,
                "wait": false,
            }),
        ),
        (
            b"M191 S50".as_slice(),
            serde_json::json!({
                "action": "set_chamber_temperature",
                "temperature_celsius": 50,
                "wait": true,
            }),
        ),
    ] {
        let result = operation_json(message);

        assert_eq!(result.status, 0);
        assert_eq!(result.http_code, 200);
        assert_json_body_eq(result, expected);
    }
}

#[test]
fn studio_message_parser_maps_light_nodes_to_semantic_json() {
    for (message, expected) in [
        (
            br#"{"system":{"command":"ledctrl","led_node":"chamber_light","led_mode":"on","sequence_id":"1"}}"#.as_slice(),
            serde_json::json!({"action":"set_chamber_light","light_on":true}),
        ),
        (
            br#"{"system":{"command":"ledctrl","led_node":"chamber_light2","led_mode":"off","sequence_id":"2"}}"#.as_slice(),
            serde_json::json!({"action":"set_chamber_light","light_on":false}),
        ),
    ] {
        let result = operation_json(message);

        assert_eq!(result.status, 0);
        assert_eq!(result.http_code, 200);
        assert_json_body_eq(result, expected);
    }
}

#[test]
fn gcode_parser_rejects_unsupported_or_ambiguous_commands() {
    for message in [
        b"G0 X10".as_slice(),
        b"G90\nG0 X10",
        b"G91 X10\nG0 Y5",
        b"G91 F3000\nG0 X5",
        b"G91\nG0 X1 E2",
        b"G91\nG0 X1\nG90",
        b"G91\nG0 X1\nM104 S200",
    ] {
        let result = operation_json(message);

        assert_ne!(result.status, 0);
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"unsupported_printer_operation"}"#);
    }
}
