use super::support::*;

pub(crate) fn studio_message_parser_maps_light_nodes_to_semantic_json() {
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

pub(crate) fn studio_message_parser_maps_print_commands_to_semantic_json() {
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

pub(crate) fn operation_parser_maps_modern_studio_axis_commands_to_required_features() {
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

pub(crate) fn operation_parser_rejects_invalid_modern_studio_axis_commands() {
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

pub(crate) fn operation_parser_maps_legacy_studio_gcode_wrappers_without_required_features() {
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
        (
            "M104 S210\n",
            serde_json::json!({
                "action": "set_hotend_temperature",
                "temperature_celsius": 210,
                "wait": false
            }),
        ),
        (
            "M109 S215\n",
            serde_json::json!({
                "action": "set_hotend_temperature",
                "temperature_celsius": 215,
                "wait": true
            }),
        ),
        (
            "M140 S60\n",
            serde_json::json!({
                "action": "set_bed_temperature",
                "temperature_celsius": 60,
                "wait": false
            }),
        ),
        (
            "M190 S65\n",
            serde_json::json!({
                "action": "set_bed_temperature",
                "temperature_celsius": 65,
                "wait": true
            }),
        ),
        (
            "M141 S45\n",
            serde_json::json!({
                "action": "set_chamber_temperature",
                "temperature_celsius": 45,
                "wait": false
            }),
        ),
        (
            "M191 S50\n",
            serde_json::json!({
                "action": "set_chamber_temperature",
                "temperature_celsius": 50,
                "wait": true
            }),
        ),
    ] {
        let message = studio_gcode_line_message(gcode);
        assert_operation_json_eq(operation_json(&message), expected);
    }
}

pub(crate) fn operation_parser_falls_back_unknown_studio_gcode_line_exactly() {
    for param in ["M106 P1 S127 \n", "M620 C1 \r\n; keep trailing  \n\n", ""] {
        let result = operation_json(&studio_gcode_line_message(param));

        assert_operation_json_eq(
            result,
            serde_json::json!({"action": "gcode_line", "param": param}),
        );
    }
}

pub(crate) fn operation_parser_rejects_non_string_studio_gcode_line_params() {
    for print in [
        serde_json::json!({"command": "gcode_line", "sequence_id": "42"}),
        serde_json::json!({"command": "gcode_line", "param": null, "sequence_id": "42"}),
        serde_json::json!({"command": "gcode_line", "param": true, "sequence_id": "42"}),
        serde_json::json!({"command": "gcode_line", "param": 127, "sequence_id": "42"}),
        serde_json::json!({"command": "gcode_line", "param": ["M106"], "sequence_id": "42"}),
        serde_json::json!({"command": "gcode_line", "param": {"line": "M106"}, "sequence_id": "42"}),
    ] {
        let result = operation_json(&studio_print_message(print));

        assert_ne!(result.status, 0);
        assert_eq!(result.http_code, 400);
        assert_eq!(body(result), r#"{"error":"unsupported_printer_operation"}"#);
    }
}
