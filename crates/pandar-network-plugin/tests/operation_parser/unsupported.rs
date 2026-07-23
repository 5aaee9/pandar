use super::support::*;

pub(crate) fn every_finite_non_firmware_unsupported_pair_returns_the_stable_parser_error() {
    let command_groups: [(&str, &[&str]); 5] = [
        (
            "camera",
            &[
                "ipcam_cap_pic_set",
                "ipcam_delete_oldest_timelapse",
                "ipcam_get_media_info",
                "ipcam_record_set",
                "ipcam_resolution_set",
                "ipcam_timelapse",
            ],
        ),
        (
            "print",
            &[
                "ams_control",
                "ams_filament_drying",
                "ams_filament_setting",
                "ams_reset",
                "ams_user_setting",
                "auto_stop_ams_dry",
                "buzzer_ctrl",
                "calibration",
                "clean_print_error",
                "close_air_filt",
                "extrusion_cali",
                "extrusion_cali_del",
                "extrusion_cali_get",
                "extrusion_cali_get_result",
                "extrusion_cali_sel",
                "extrusion_cali_set",
                "flowrate_cali",
                "flowrate_get_result",
                "gcode_file",
                "get_auto_nozzle_mapping",
                "holder_nozzle_refresh",
                "idle_ignore",
                "nozzle_holder_ctrl",
                "nozzle_info_confirm",
                "print_option",
                "refresh_nozzle",
                "set_against_continued_heating_mode",
                "set_airduct",
                "set_extrusion_length",
                "set_fan",
                "skip_objects",
            ],
        ),
        ("pushing", &["start", "stop"]),
        (
            "system",
            &[
                "get_access_code",
                "print_cache_set",
                "set_door_stat",
                "uiop",
            ],
        ),
        ("xcam", &["xcam_control_set"]),
    ];
    assert_eq!(
        command_groups
            .iter()
            .map(|(_, commands)| commands.len())
            .sum::<usize>(),
        44,
        "the firmware parser owns the 45th unsupported pair"
    );

    let mut tested_pairs = std::collections::BTreeSet::new();
    for (envelope, commands) in command_groups {
        for command in commands {
            let mut message = serde_json::Map::new();
            message.insert(
                envelope.to_owned(),
                serde_json::json!({"command": command, "sequence_id": "123"}),
            );
            let case = format!("{envelope}.{command}");
            assert!(tested_pairs.insert(case.clone()), "duplicate pair: {case}");
            let message = serde_json::to_vec(&message).unwrap();

            assert_stable_unsupported(operation_json(&message), &case);
        }
    }
    assert_eq!(tested_pairs.len(), 44);
}

pub(crate) fn unknown_malformed_and_mixed_studio_messages_return_the_stable_parser_error() {
    let cases = [
        (
            "unknown envelope",
            serde_json::json!({"future": {"command": "future_control", "sequence_id": "123"}}),
        ),
        (
            "malformed ordinary command",
            serde_json::json!({"print": {"command": "set_nozzle_temp", "extruder_index": "left", "target_temp": 245, "sequence_id": "123"}}),
        ),
        (
            "mixed system and print envelopes",
            serde_json::json!({
                "system": {
                    "command": "ledctrl",
                    "led_node": "chamber_light",
                    "led_mode": "on",
                    "sequence_id": "123"
                },
                "print": {"command": "pause", "sequence_id": "124"}
            }),
        ),
    ];

    for (name, value) in cases {
        let message = serde_json::to_vec(&value).unwrap();
        assert_stable_unsupported(operation_json(&message), name);
    }
}

pub(crate) fn operation_parser_requires_the_exact_legacy_studio_axis_envelope() {
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
        let param = format!("{}\n", commands.join("\n"));
        let message = studio_gcode_line_message(&param);
        let result = operation_json(&message);

        assert_operation_json_eq(
            result,
            serde_json::json!({"action": "gcode_line", "param": param}),
        );
    }

    let param = r#"{"print":{"command":"gcode_line","param":"G28 X"}}"#;
    let recursive = studio_gcode_line_message(param);
    assert_operation_json_eq(
        operation_json(&recursive),
        serde_json::json!({"action": "gcode_line", "param": param}),
    );
}
