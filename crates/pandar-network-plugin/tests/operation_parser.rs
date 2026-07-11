use pandar_network_plugin::{
    PluginHttpResult, pandar_plugin_free_with_capacity, pandar_plugin_operation_json_from_gcode,
};
use serde::Deserialize;

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

fn assert_operation_body_eq(result: PluginHttpResult, expected: TestOperation) {
    let actual: TestOperation = serde_json::from_str(&body(result)).unwrap();
    assert_eq!(actual, expected);
}

fn assert_operation_json_eq(result: PluginHttpResult, expected: serde_json::Value) {
    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body(result)).unwrap(),
        expected
    );
}

fn studio_print_message(print: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"print": print})).unwrap()
}

fn studio_gcode_line_message(gcode: &str) -> Vec<u8> {
    studio_print_message(serde_json::json!({
        "command": "gcode_line",
        "param": gcode,
        "sequence_id": "42"
    }))
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
enum TestOperation {
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
struct TestAxisMovement {
    axis: String,
    delta_mm: f64,
}

#[test]
fn gcode_parser_maps_home_and_axes_to_semantic_json() {
    let result = operation_json(b"  G28 X Z ; home selected axes\n");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_operation_body_eq(
        result,
        TestOperation::Home {
            axes: vec!["x".to_owned(), "z".to_owned()],
        },
    );
}

#[test]
fn gcode_parser_maps_relative_move_to_semantic_json() {
    let result = operation_json(b"G91\nG0 X10.5 Z-0.25 F3000");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_operation_body_eq(
        result,
        TestOperation::MoveAxes {
            movements: vec![
                TestAxisMovement {
                    axis: "x".to_owned(),
                    delta_mm: 10.5,
                },
                TestAxisMovement {
                    axis: "z".to_owned(),
                    delta_mm: -0.25,
                },
            ],
            feedrate_mm_per_min: 3000,
        },
    );
}

#[test]
fn gcode_parser_maps_hotend_temperature_to_semantic_json() {
    let result = operation_json(b"M109 S215");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_operation_body_eq(
        result,
        TestOperation::SetHotendTemperature {
            temperature_celsius: 215,
            wait: true,
            extruder_id: None,
        },
    );
}

#[test]
fn gcode_parser_maps_targeted_hotend_temperature_to_semantic_json() {
    let result = operation_json(b"M104 S210 T1");

    assert_eq!(result.status, 0);
    assert_eq!(result.http_code, 200);
    assert_operation_body_eq(
        result,
        TestOperation::SetHotendTemperature {
            temperature_celsius: 210,
            wait: false,
            extruder_id: Some(1),
        },
    );
}

#[test]
fn gcode_parser_maps_bed_and_chamber_temperature_to_semantic_json() {
    for (message, expected) in [
        (
            b"M140 S60".as_slice(),
            TestOperation::SetBedTemperature {
                temperature_celsius: 60,
                wait: false,
            },
        ),
        (
            b"M190 S65".as_slice(),
            TestOperation::SetBedTemperature {
                temperature_celsius: 65,
                wait: true,
            },
        ),
        (
            b"M141 S45".as_slice(),
            TestOperation::SetChamberTemperature {
                temperature_celsius: 45,
                wait: false,
            },
        ),
        (
            b"M191 S50".as_slice(),
            TestOperation::SetChamberTemperature {
                temperature_celsius: 50,
                wait: true,
            },
        ),
    ] {
        let result = operation_json(message);

        assert_eq!(result.status, 0);
        assert_eq!(result.http_code, 200);
        assert_operation_body_eq(result, expected);
    }
}

#[test]
fn studio_message_parser_maps_light_nodes_to_semantic_json() {
    for (message, expected) in [
        (
            br#"{"system":{"command":"ledctrl","led_node":"chamber_light","led_mode":"on","sequence_id":"1"}}"#.as_slice(),
            TestOperation::SetChamberLight { light_on: true },
        ),
        (
            br#"{"system":{"command":"ledctrl","led_node":"chamber_light2","led_mode":"off","sequence_id":"2"}}"#.as_slice(),
            TestOperation::SetChamberLight { light_on: false },
        ),
    ] {
        let result = operation_json(message);

        assert_eq!(result.status, 0);
        assert_eq!(result.http_code, 200);
        assert_operation_body_eq(result, expected);
    }
}

#[test]
fn studio_message_parser_maps_print_commands_to_semantic_json() {
    for (message, expected) in [
        (
            br#"{"print":{"command":"pause","param":"","sequence_id":"1"}}"#.as_slice(),
            TestOperation::Pause,
        ),
        (
            br#"{"print":{"command":"resume","param":"","sequence_id":"2"}}"#.as_slice(),
            TestOperation::Resume,
        ),
        (
            br#"{"print":{"command":"stop","param":"","job_id":"job","sequence_id":"3"}}"#.as_slice(),
            TestOperation::Stop,
        ),
        (
            br#"{"print":{"command":"print_speed","param":"3","sequence_id":"4"}}"#.as_slice(),
            TestOperation::SetPrintSpeed { speed_mode: 3 },
        ),
        (
            br#"{"print":{"command":"select_extruder","extruder_index":1,"sequence_id":"5"}}"#
                .as_slice(),
            TestOperation::SelectExtruder { extruder_id: 1 },
        ),
        (
            br#"{"print":{"command":"set_nozzle_temp","extruder_index":1,"target_temp":245,"sequence_id":"6"}}"#
                .as_slice(),
            TestOperation::SetHotendTemperature {
                temperature_celsius: 245,
                wait: false,
                extruder_id: Some(1),
            },
        ),
        (
            br#"{"print":{"command":"set_bed_temp","temp":65,"sequence_id":"7"}}"#.as_slice(),
            TestOperation::SetBedTemperature {
                temperature_celsius: 65,
                wait: false,
            },
        ),
        (
            br#"{"print":{"command":"set_ctt","ctt_val":45,"sequence_id":"8"}}"#.as_slice(),
            TestOperation::SetChamberTemperature {
                temperature_celsius: 45,
                wait: false,
            },
        ),
        (
            br#"{"print":{"command":"ams_get_rfid","ams_id":1,"slot_id":2,"sequence_id":"9"}}"#
                .as_slice(),
            TestOperation::AmsRereadRfid {
                ams_id: 1,
                slot_id: 2,
            },
        ),
        (
            br#"{"print":{"command":"ams_change_filament","ams_id":1,"slot_id":2,"target":6,"curr_temp":210,"tar_temp":220,"extruder_id":0,"sequence_id":"10"}}"#
                .as_slice(),
            TestOperation::AmsLoadFilament {
                ams_id: 1,
                slot_id: 2,
                global_tray_id: 6,
                extruder_id: Some(0),
            },
        ),
        (
            br#"{"print":{"command":"ams_change_filament","ams_id":1,"slot_id":255,"target":255,"curr_temp":210,"tar_temp":210,"sequence_id":"11"}}"#
                .as_slice(),
            TestOperation::AmsUnloadFilament {
                ams_id: 1,
                slot_id: 255,
            },
        ),
    ] {
        let result = operation_json(message);

        assert_eq!(result.status, 0);
        assert_eq!(result.http_code, 200);
        assert_operation_body_eq(result, expected);
    }
}

#[test]
fn operation_parser_maps_modern_studio_axis_commands_to_required_features() {
    for (print, expected) in [
        (
            serde_json::json!({"command": "back_to_center", "sequence_id": "1"}),
            serde_json::json!({
                "action": "home",
                "axes": [],
                "required_device_features": ["bambu_mqtt_homing"]
            }),
        ),
        (
            serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": 1, "mode": 0, "sequence_id": "2"}),
            serde_json::json!({
                "action": "move_axes",
                "movements": [{"axis": "x", "delta_mm": 1.0}],
                "required_device_features": ["bambu_mqtt_axis_control"]
            }),
        ),
        (
            serde_json::json!({"command": "xyz_ctrl", "axis": "Y", "dir": -1, "mode": 1, "sequence_id": "3"}),
            serde_json::json!({
                "action": "move_axes",
                "movements": [{"axis": "y", "delta_mm": -10.0}],
                "required_device_features": ["bambu_mqtt_axis_control"]
            }),
        ),
        (
            serde_json::json!({"command": "xyz_ctrl", "axis": "Z", "dir": 1, "mode": 1, "sequence_id": "4"}),
            serde_json::json!({
                "action": "move_axes",
                "movements": [{"axis": "z", "delta_mm": 10.0}],
                "required_device_features": ["bambu_mqtt_axis_control"]
            }),
        ),
    ] {
        let message = studio_print_message(print);
        assert_operation_json_eq(operation_json(&message), expected);
    }
}

#[test]
fn operation_parser_rejects_invalid_modern_studio_axis_commands() {
    for print in [
        serde_json::json!({"command": "xyz_ctrl", "axis": "x", "dir": 1, "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "E", "dir": 1, "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": 0, "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": 2, "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": "1", "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": 1, "mode": 2}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": 1, "mode": "0"}),
        serde_json::json!({"command": "xyz_ctrl", "dir": 1, "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "mode": 0}),
        serde_json::json!({"command": "xyz_ctrl", "axis": "X", "dir": 1}),
    ] {
        let message = studio_print_message(print);
        let result = operation_json(&message);

        assert_ne!(result.status, 0);
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"unsupported_printer_operation"}"#);
    }
}

#[test]
fn operation_parser_maps_legacy_studio_gcode_wrappers_without_required_features() {
    for (gcode, expected) in [
        ("G28\n", serde_json::json!({"action": "home", "axes": []})),
        (
            "G28 X\n",
            serde_json::json!({"action": "home", "axes": ["x"]}),
        ),
        (
            "G28 Z X Y\n",
            serde_json::json!({"action": "home", "axes": ["z", "x", "y"]}),
        ),
        (
            "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 X10.0 F3000\nM1002 pop_ref_mode\nM211 R\n",
            serde_json::json!({
                "action": "move_axes",
                "movements": [{"axis": "x", "delta_mm": 10.0}],
                "feedrate_mm_per_min": 3000
            }),
        ),
        (
            "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 Z-1.0 F600\nM1002 pop_ref_mode\nM211 R\n",
            serde_json::json!({
                "action": "move_axes",
                "movements": [{"axis": "z", "delta_mm": -1.0}],
                "feedrate_mm_per_min": 600
            }),
        ),
    ] {
        let message = studio_gcode_line_message(gcode);
        assert_operation_json_eq(operation_json(&message), expected);
    }
}

#[test]
fn operation_parser_requires_the_exact_legacy_studio_axis_envelope() {
    let envelope = [
        "M211 S",
        "M211 X1 Y1 Z1",
        "M1002 push_ref_mode",
        "G91",
        "G1 X10.0 F3000",
        "M1002 pop_ref_mode",
        "M211 R",
    ];
    let altered = [
        "M211 T",
        "M211 X1 Y1 Z0",
        "M1002 push_mode",
        "G90",
        "G0 X10.0 F3000",
        "M1002 pop_mode",
        "M211 S",
    ];
    let mut rejected = Vec::new();
    for omitted in 0..envelope.len() {
        rejected.push(
            envelope
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != omitted)
                .map(|(_, line)| *line)
                .collect::<Vec<_>>(),
        );
    }
    for changed in 0..envelope.len() {
        let mut commands = envelope.to_vec();
        commands[changed] = altered[changed];
        rejected.push(commands);
    }
    let mut reordered = envelope.to_vec();
    reordered.swap(0, 1);
    rejected.push(reordered);
    let mut extra = envelope.to_vec();
    extra.push("M400");
    rejected.push(extra);

    for commands in rejected {
        let message = studio_gcode_line_message(&format!("{}\n", commands.join("\n")));
        let result = operation_json(&message);

        assert_ne!(result.status, 0, "unexpectedly accepted {commands:?}");
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"unsupported_printer_operation"}"#);
    }

    let recursive =
        studio_gcode_line_message(r#"{"print":{"command":"gcode_line","param":"G28 X"}}"#);
    let result = operation_json(&recursive);
    assert_ne!(result.status, 0);
    assert_eq!(result.http_code, 400);
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
