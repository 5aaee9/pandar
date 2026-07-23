use super::*;

#[test]
fn studio_command_payloads_use_incrementing_studio_sequence_ids() {
    let commands = [
        (BambuMqttCommand::GetVersion.payload(), "info"),
        (BambuMqttCommand::RequestPushAll.payload(), "pushing"),
        (BambuMqttCommand::PausePrint.payload(), "print"),
        (BambuMqttCommand::ResumePrint.payload(), "print"),
        (BambuMqttCommand::StopPrint.payload(), "print"),
        (BambuMqttCommand::SetChamberLight(true).payload(), "system"),
        (
            BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(4).unwrap()).payload(),
            "print",
        ),
        (
            BambuMqttCommand::GcodeLine(GcodeLineCommand {
                param: "G28".to_string(),
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::project_file(ProjectFileCommand {
                flow_cali: true,
                ..project_file_command()
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::AmsRereadRfid(AmsSlotCommand {
                ams_id: 0,
                slot_id: 1,
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::AmsLoadFilament(AmsFilamentCommand {
                ams_id: 0,
                slot_id: 1,
                target: 1,
                extruder_id: Some(0),
            })
            .payload(),
            "print",
        ),
        (
            BambuMqttCommand::AmsUnloadFilament(AmsFilamentCommand {
                ams_id: 0,
                slot_id: 1,
                target: 1,
                extruder_id: None,
            })
            .payload(),
            "print",
        ),
    ];

    let command_count = commands.len();
    let mut sequence_ids = Vec::new();
    for (payload, section) in commands {
        sequence_ids.push(studio_sequence_id(&payload, section));
    }
    sequence_ids.sort();
    sequence_ids.dedup();
    assert_eq!(sequence_ids.len(), command_count);
}

#[test]
fn studio_sequence_id_wraps_before_leaving_studio_range() {
    let sequence = AtomicU32::new(29999);

    let last = next_studio_sequence_id_from(&sequence);
    let wrapped = next_studio_sequence_id_from(&sequence);
    let continued = next_studio_sequence_id_from(&sequence);

    assert_eq!(last, "29999");
    assert_eq!(wrapped, "20000");
    assert_eq!(continued, "20001");

    let out_of_range = AtomicU32::new(30000);
    assert_eq!(next_studio_sequence_id_from(&out_of_range), "20000");
    assert_eq!(next_studio_sequence_id_from(&out_of_range), "20001");
}

#[test]
fn topics_match_bambu_reference_shape() {
    let topics = BambuMqttTopics::for_serial("01S00EXAMPLE");

    assert_eq!(topics.report, "device/01S00EXAMPLE/report");
    assert_eq!(topics.request, "device/01S00EXAMPLE/request");
}

#[test]
fn constants_match_bambu_defaults() {
    assert_eq!(BAMBU_MQTT_PORT, 8883);
    assert_eq!(BAMBU_MQTT_USERNAME, "bblp");
    assert_eq!(BAMBU_MQTT_QOS, 1);
}

#[test]
fn lan_mqtt_accepts_full_pushall_reports() {
    let options = bambu_lan_mqtt_options(&endpoint(), None);

    assert!(options.max_packet_size() >= 256 * 1024);
}

#[test]
fn mqtt_report_error_log_preserves_error_chain() {
    let err = anyhow!("payload size limit exceeded: 262600")
        .context("MQTT serialization/deserialization error")
        .context("poll rumqttc event loop");

    let (logs, ()) = crate::test_tracing::capture_logs(|| warn_mqtt_report_receive_failed(&err));

    let captured = logs.contents();
    assert!(captured.contains("MQTT report receive failed"));
    assert!(captured.contains("payload size limit exceeded: 262600"));
    assert!(captured.contains("poll rumqttc event loop"));
}

#[test]
fn lan_tls_uses_rustls_certificate_policy_for_printer_certificates() {
    assert!(matches!(
        bambu_lan_tls_config(),
        TlsConfiguration::Rustls(_)
    ));
}

#[test]
fn ftps_lan_tls_default_profile_config_constructs() {
    let config = crate::machine::ftps::bambu_lan_ftps_tls_config_for_default_profile();

    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn pushall_payload_matches_reference() {
    let payload = BambuMqttCommand::RequestPushAll.payload();
    let sequence_id = studio_sequence_id(&payload, "pushing");
    assert_eq!(payload, expected_pushall_payload(&sequence_id));
}

#[test]
fn get_version_payload_matches_reference() {
    let payload = BambuMqttCommand::GetVersion.payload();
    let sequence_id = studio_sequence_id(&payload, "info");
    assert_eq!(payload, expected_get_version_payload(&sequence_id));
}

#[test]
fn get_version_report_extracts_trimmed_ota_model() {
    let observation = parse_firmware_version_observation(&get_version_report(" P2S "))
        .unwrap()
        .unwrap();
    assert_eq!(observation.model, "P2S");
}

#[test]
fn get_version_report_rejects_missing_model() {
    let report = get_version_report_with_blank_model();

    assert!(parse_firmware_version_observation(&report).is_err());
}

#[test]
fn basic_print_control_payloads_match_reference() {
    let pause = BambuMqttCommand::PausePrint.payload();
    let resume = BambuMqttCommand::ResumePrint.payload();
    let stop = BambuMqttCommand::StopPrint.payload();
    assert_eq!(
        pause,
        expected_print_command_payload("pause", "", &studio_sequence_id(&pause, "print"))
    );
    assert_eq!(
        resume,
        expected_print_command_payload("resume", "", &studio_sequence_id(&resume, "print"))
    );
    assert_eq!(
        stop,
        expected_print_command_payload("stop", "", &studio_sequence_id(&stop, "print"))
    );
}

#[test]
fn chamber_light_payload_matches_bambu_studio_reference() {
    let on = BambuMqttCommand::SetChamberLight(true).payload();
    assert_eq!(
        on,
        expected_chamber_light_payload(&studio_sequence_id(&on, "system"))
    );

    let off = BambuMqttCommand::SetChamberLight(false).payload();
    assert_eq!(chamber_light_payload(&off).system.led_mode, "off");
}

#[test]
fn print_speed_is_limited_to_reference_modes() {
    let payload = BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(4).unwrap()).payload();
    assert_eq!(
        payload,
        expected_print_command_payload("print_speed", "4", &studio_sequence_id(&payload, "print"))
    );
    assert!(PrintSpeed::new(0).is_err());
    assert!(PrintSpeed::new(5).is_err());
}

#[test]
fn select_extruder_payload_matches_bambu_studio_reference() {
    let payload = BambuMqttCommand::SelectExtruder(1).payload();
    assert_eq!(
        payload,
        expected_select_extruder_payload(1, &studio_sequence_id(&payload, "print"))
    );
}

#[test]
fn axis_controls_back_to_center_payload_is_typed_and_exact() {
    let payload = BambuMqttCommand::BackToCenter.payload();
    assert_eq!(
        payload,
        serde_json::json!({
            "print": {
                "command": "back_to_center",
                "sequence_id": studio_sequence_id(&payload, "print"),
            }
        })
    );
}

#[test]
fn axis_controls_xyz_control_payload_uses_uppercase_axis_and_numeric_fields() {
    for (axis, direction, mode, expected_axis) in [
        (crate::machine::PrinterAxis::X, 1, 0, "X"),
        (crate::machine::PrinterAxis::Y, -1, 1, "Y"),
        (crate::machine::PrinterAxis::Z, 1, 1, "Z"),
    ] {
        let payload = BambuMqttCommand::XyzControl {
            axis,
            direction,
            mode,
        }
        .payload();
        assert_eq!(
            payload,
            serde_json::json!({
                "print": {
                    "command": "xyz_ctrl",
                    "axis": expected_axis,
                    "dir": direction,
                    "mode": mode,
                    "sequence_id": studio_sequence_id(&payload, "print"),
                }
            })
        );
    }
}

#[test]
fn gcode_line_payload_preserves_single_home_line() {
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        param: "G28".to_string(),
    })
    .payload();
    assert_eq!(
        payload,
        expected_print_command_payload("gcode_line", "G28", &studio_sequence_id(&payload, "print"))
    );
}

#[test]
fn gcode_line_payload_preserves_studio_axis_move_envelope() {
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        param: "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 X10 Z-0.5 F3000\nM1002 pop_ref_mode\nM211 R".to_string(),
    })
    .payload();
    assert_eq!(
        payload,
        expected_print_command_payload(
            "gcode_line",
            "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG1 X10 Z-0.5 F3000\nM1002 pop_ref_mode\nM211 R",
            &studio_sequence_id(&payload, "print")
        )
    );
}

#[test]
fn gcode_line_payload_preserves_hotend_temperature_line() {
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        param: "M104 S200".to_string(),
    })
    .payload();
    assert_eq!(
        payload,
        expected_print_command_payload(
            "gcode_line",
            "M104 S200",
            &studio_sequence_id(&payload, "print")
        )
    );
}

#[test]
fn gcode_line_payload_preserves_exact_param() {
    let param = "M106 P1 S127 \r\n; keep  \n\n";
    let payload = BambuMqttCommand::GcodeLine(GcodeLineCommand {
        param: param.to_owned(),
    })
    .command_payload();

    assert_eq!(payload.payload["print"]["param"], param);
}

#[test]
fn raw_json_payload_is_preserved() {
    let payload = raw_print_payload("custom", "9");
    assert_eq!(
        BambuMqttCommand::RawJson(payload.clone()).payload(),
        payload
    );
}
