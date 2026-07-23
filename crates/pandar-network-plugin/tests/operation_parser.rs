#[path = "operation_parser/gcode.rs"]
mod gcode;
#[path = "operation_parser/studio.rs"]
mod studio;
#[path = "operation_parser/support.rs"]
mod support;
#[path = "operation_parser/unsupported.rs"]
mod unsupported;

macro_rules! test_case {
    ($name:ident, $module:ident) => {
        #[test]
        fn $name() {
            $module::$name();
        }
    };
}

test_case!(gcode_parser_maps_home_and_axes_to_semantic_json, gcode);
test_case!(gcode_parser_maps_relative_move_to_semantic_json, gcode);
test_case!(gcode_parser_maps_hotend_temperature_to_semantic_json, gcode);
test_case!(
    gcode_parser_maps_targeted_hotend_temperature_to_semantic_json,
    gcode
);
test_case!(
    gcode_parser_maps_bed_and_chamber_temperature_to_semantic_json,
    gcode
);
test_case!(
    studio_message_parser_maps_light_nodes_to_semantic_json,
    studio
);
test_case!(
    studio_message_parser_maps_print_commands_to_semantic_json,
    studio
);
test_case!(
    operation_parser_maps_modern_studio_axis_commands_to_required_features,
    studio
);
test_case!(
    operation_parser_rejects_invalid_modern_studio_axis_commands,
    studio
);
test_case!(
    operation_parser_maps_legacy_studio_gcode_wrappers_without_required_features,
    studio
);
test_case!(
    operation_parser_falls_back_unknown_studio_gcode_line_exactly,
    studio
);
test_case!(
    operation_parser_rejects_non_string_studio_gcode_line_params,
    studio
);
test_case!(
    every_finite_non_firmware_unsupported_pair_returns_the_stable_parser_error,
    unsupported
);
test_case!(
    unknown_malformed_and_mixed_studio_messages_return_the_stable_parser_error,
    unsupported
);
test_case!(
    operation_parser_requires_the_exact_legacy_studio_axis_envelope,
    unsupported
);
test_case!(
    gcode_parser_rejects_unsupported_or_ambiguous_commands,
    gcode
);
