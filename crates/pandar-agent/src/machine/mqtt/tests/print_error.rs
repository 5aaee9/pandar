use super::*;

#[test]
fn native_print_error_actions_match_studio_payloads() {
    for (action, command) in [
        (PrintErrorAction::Resume, "resume"),
        (PrintErrorAction::Ignore, "ignore"),
        (PrintErrorAction::Stop, "stop"),
    ] {
        let payload = BambuMqttCommand::HandlePrintError(HandlePrintErrorCommand {
            error_action: action,
            print_error: 83_918_929,
            printer_job_id: "job-7".to_owned(),
            sequence_id: 20_042,
        })
        .command_payload();
        assert_eq!(payload.sequence_id.as_deref(), Some("20042"));
        assert_eq!(
            payload.payload,
            serde_json::json!({
                "print": {
                    "command": command,
                    "err": "83918929",
                    "job_id": "job-7",
                    "param": "reserve",
                    "sequence_id": "20042"
                }
            })
        );
    }
}

#[test]
fn numeric_print_error_matches_studio_int_state_semantics() {
    let cases = [
        (serde_json::json!(0), Some(0)),
        (serde_json::json!(-3), Some(0)),
        (serde_json::json!(12.9), Some(12)),
        (serde_json::json!(i32::MAX), Some(i32::MAX as u32)),
        (serde_json::json!(2147483648_u64), None),
    ];
    for (value, expected) in cases {
        let progress = print_report_from_json(
            &endpoint(),
            &serde_json::json!({"print": {"print_error": value, "mc_percent": 37}}),
        );
        assert_eq!(progress.print_error, expected);
        assert_eq!(progress.percent, Some(37));
    }
}

#[test]
fn x2d_numeric_legacy_state_does_not_drop_recovery_fields() {
    let progress = print_report_from_json(
        &endpoint(),
        &serde_json::json!({
            "print": {
                "gcode_state": "PAUSE",
                "state": 2,
                "print_error": 83_918_946,
                "job_attr": 18,
                "hms": [{"attr": 83_952_640, "code": 196_610}]
            }
        }),
    );

    assert_eq!(progress.gcode_state.as_deref(), Some("PAUSE"));
    assert_eq!(progress.print_error, Some(83_918_946));
    assert_eq!(progress.job_attr, Some(18));
    assert_eq!(progress.hms.as_deref().map(|hms| hms.len()), Some(1));
}

#[test]
fn zero_print_error_is_state_not_a_generic_diagnostic() {
    let progress = print_report_from_json(
        &endpoint(),
        &serde_json::json!({"print": {"print_error": 0}}),
    );
    assert_eq!(progress.print_error, Some(0));
    assert!(progress.diagnostics.is_empty());
}

#[test]
fn printer_job_id_preserves_presence_and_studio_conversion() {
    let cases = [
        (serde_json::json!(""), Some(String::new())),
        (
            serde_json::json!("not-a-number"),
            Some("not-a-number".to_owned()),
        ),
        (serde_json::json!(42.9), Some("42".to_owned())),
        (
            serde_json::json!(9223372036854775808_u64),
            Some(String::new()),
        ),
        (serde_json::json!({"bad": true}), Some(String::new())),
    ];
    for (value, expected) in cases {
        let progress = print_report_from_json(
            &endpoint(),
            &serde_json::json!({"print": {"task_id": "task-7", "job_id": value}}),
        );
        assert_eq!(progress.job_id.as_deref(), Some("task-7"));
        assert_eq!(progress.printer_job_id, expected);
    }
}

#[test]
fn raw_mqtt_printer_job_id_numbers_keep_decimal_boundary_semantics() {
    let cases = [
        (
            "i64-min",
            "-9223372036854775808",
            Some("-9223372036854775808"),
        ),
        (
            "i64-max",
            "9223372036854775807",
            Some("9223372036854775807"),
        ),
        ("below-i64-min", "-9223372036854775809", Some("")),
        ("above-i64-max", "9223372036854775808", Some("")),
        (
            "positive-fraction-in-range",
            "9223372036854775807.9",
            Some("9223372036854775807"),
        ),
        (
            "positive-fraction-out-of-range",
            "9223372036854775808.0",
            Some(""),
        ),
        (
            "negative-fraction-in-range",
            "-9223372036854775807.9",
            Some("-9223372036854775807"),
        ),
        (
            "negative-fraction-out-of-range",
            "-9223372036854775808.1",
            Some(""),
        ),
        (
            "positive-exponent",
            "9.223372036854775807e+18",
            Some("9223372036854775807"),
        ),
        (
            "negative-exponent-out-of-range",
            "-9.223372036854775809e18",
            Some(""),
        ),
        ("huge-exponent-out-of-range", "1e400", Some("")),
        ("fractional-exponent", "4.29e1", Some("42")),
        ("string", r#""00123""#, Some("00123")),
        (
            "nonnumeric-string",
            r#""not-a-number""#,
            Some("not-a-number"),
        ),
        ("null", "null", Some("")),
        ("bool", "true", Some("")),
        ("object", r#"{"bad":true}"#, Some("")),
    ];

    let actual = cases
        .iter()
        .map(|(name, raw_job_id, _)| {
            let raw = format!(r#"{{"print":{{"job_id":{raw_job_id},"mc_percent":37}}}}"#);
            let report = decode_mqtt_report_payload(raw.as_bytes()).expect("valid raw MQTT JSON");
            let progress = print_report_from_json(&endpoint(), &report);
            (*name, progress.printer_job_id, progress.percent)
        })
        .collect::<Vec<_>>();
    let expected = cases
        .iter()
        .map(|(name, _, job_id)| (*name, job_id.map(ToOwned::to_owned), Some(37)))
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);

    let report =
        decode_mqtt_report_payload(br#"{"print":{"mc_percent":37}}"#).expect("valid raw MQTT JSON");
    let progress = print_report_from_json(&endpoint(), &report);
    assert_eq!(progress.printer_job_id, None);
    assert_eq!(progress.percent, Some(37));
}

#[test]
fn absent_print_error_and_printer_job_id_remain_absent() {
    let progress = print_report_from_json(
        &endpoint(),
        &serde_json::json!({"print": {"task_id": "task-7"}}),
    );

    assert_eq!(progress.print_error, None);
    assert_eq!(progress.printer_job_id, None);
}

#[test]
fn job_attr_preserves_zero_nonzero_and_absence() {
    let cases = [
        (serde_json::json!({"print": {"job_attr": 0}}), Some(0)),
        (serde_json::json!({"print": {"job_attr": 0x21}}), Some(0x21)),
        (serde_json::json!({"print": {"mc_percent": 7}}), None),
        (serde_json::json!({"print": {"job_attr": -1}}), None),
        (serde_json::json!({"print": {"job_attr": "invalid"}}), None),
    ];
    for (report, expected) in cases {
        let progress = print_report_from_json(&endpoint(), &report);
        assert_eq!(progress.job_attr, expected);
    }
}

#[test]
fn invalid_job_attr_does_not_discard_other_valid_fields() {
    let progress = print_report_from_json(
        &endpoint(),
        &serde_json::json!({"print": {"job_attr": "invalid", "mc_percent": 7}}),
    );

    assert_eq!(progress.job_attr, None);
    assert_eq!(progress.percent, Some(7));
}

#[test]
fn structured_job_attr_does_not_discard_valid_sibling_fields() {
    for job_attr in [
        serde_json::json!(true),
        serde_json::json!([1, 2, 3]),
        serde_json::json!({"unexpected": 1}),
    ] {
        let progress = print_report_from_json(
            &endpoint(),
            &serde_json::json!({
                "print": {
                    "job_attr": job_attr,
                    "gcode_state": "RUNNING",
                    "mc_percent": 37,
                    "print_error": 0
                }
            }),
        );

        assert_eq!(
            (
                progress.job_attr,
                progress.gcode_state.as_deref(),
                progress.percent,
                progress.print_error,
            ),
            (None, Some("RUNNING"), Some(37), Some(0))
        );
    }
}

#[test]
fn job_attr_presence_round_trips_to_agent_report() {
    let explicit_zero = print_job_report_event(&config(), progress_with_job_attr(Some(0)));
    let absent = print_job_report_event(&config(), progress_with_job_attr(None));
    let Some(agent_event::Event::PrintJobReport(explicit_zero)) = explicit_zero.event else {
        panic!("expected print report");
    };
    let Some(agent_event::Event::PrintJobReport(absent)) = absent.event else {
        panic!("expected print report");
    };
    assert!(explicit_zero.has_job_attr);
    assert_eq!(explicit_zero.job_attr, 0);
    assert!(!absent.has_job_attr);
}

#[test]
fn null_printer_job_id_is_present_as_an_empty_string() {
    let progress =
        print_report_from_json(&endpoint(), &serde_json::json!({"print": {"job_id": null}}));

    assert_eq!(progress.printer_job_id, Some(String::new()));
}

#[test]
fn string_and_object_print_error_keep_generic_diagnostics() {
    let cases = [
        serde_json::json!("heater failure"),
        serde_json::json!({"code": "E1", "message": "heater failure"}),
    ];

    for value in cases {
        let progress = print_report_from_json(
            &endpoint(),
            &serde_json::json!({"print": {"print_error": value}}),
        );

        assert_eq!(progress.print_error, None);
        assert_eq!(progress.diagnostics.len(), 1);
        assert_eq!(progress.diagnostics[0].kind, "print_error");
        assert_eq!(progress.diagnostics[0].message, "heater failure");
    }
}

#[test]
fn boolean_and_null_print_error_do_not_patch_numeric_state() {
    for value in [serde_json::json!(true), serde_json::Value::Null] {
        let progress = print_report_from_json(
            &endpoint(),
            &serde_json::json!({"print": {"print_error": value, "mc_percent": 37}}),
        );

        assert_eq!(progress.print_error, None);
        assert_eq!(progress.percent, Some(37));
        assert!(progress.diagnostics.is_empty());
    }
}

#[test]
fn malformed_print_error_does_not_drop_valid_report_fields() {
    let progress = print_report_from_json(
        &endpoint(),
        &serde_json::json!({
            "print": {
                "print_error": ["bad"],
                "mc_percent": 37,
                "mc_remaining_time": 12,
                "layer_num": 4,
                "total_layer_num": 8,
                "hms": [{"attr": 7, "code": 9}],
                "ams": {
                    "ams": [{
                        "id": "0",
                        "tray": [{"id": "0", "tray_type": "PLA"}]
                    }]
                }
            }
        }),
    );

    assert_eq!(progress.print_error, None);
    assert_eq!(progress.percent, Some(37));
    assert_eq!(progress.remaining_time_minutes, Some(12));
    assert_eq!(progress.current_layer, Some(4));
    assert_eq!(progress.total_layers, Some(8));
    let hms = progress.hms.as_deref().expect("valid HMS remains present");
    assert_eq!(hms.len(), 1);
    assert_eq!(hms[0].attr, 7);
    assert_eq!(hms[0].code, 9);
    assert!(!progress.printer_materials_json.is_empty());
    assert_eq!(progress.diagnostics.len(), 1);
    assert_eq!(progress.diagnostics[0].kind, "hms");
}

#[test]
fn print_job_report_event_preserves_print_error_and_printer_job_id_presence() {
    let explicit = print_report_from_json(
        &endpoint(),
        &serde_json::json!({
            "print": {"task_id": "task-7", "print_error": 0, "job_id": ""}
        }),
    );
    let explicit = print_job_report_event(&config(), explicit);
    let Some(agent_event::Event::PrintJobReport(explicit)) = explicit.event else {
        panic!("expected print job report event");
    };

    assert_eq!(explicit.job_id, "task-7");
    assert_eq!(explicit.print_error, 0);
    assert!(explicit.has_print_error);
    assert_eq!(explicit.printer_job_id, "");
    assert!(explicit.has_printer_job_id);

    let absent = print_job_report_event(&config(), progress_with_job_attr(None));
    let Some(agent_event::Event::PrintJobReport(absent)) = absent.event else {
        panic!("expected print job report event");
    };

    assert_eq!(absent.print_error, 0);
    assert!(!absent.has_print_error);
    assert_eq!(absent.printer_job_id, "");
    assert!(!absent.has_printer_job_id);
}

fn config() -> AgentConfig {
    AgentConfig {
        hub_grpc_url: "http://hub.internal:50051".to_owned(),
        hub_api_url: None,
        agent_name: "garage".to_owned(),
        agent_id: "agent-id".to_owned(),
        tenant_id: "tenant-id".to_owned(),
        agent_credential: "pandar_ac_test".to_owned(),
        agent_version: "9.8.7".to_owned(),
        printers: "[]".to_owned(),
    }
}

fn progress_with_job_attr(job_attr: Option<u32>) -> PrintReportProgress {
    PrintReportProgress {
        serial: "01S00EXAMPLE".to_owned(),
        job_id: None,
        job_attr,
        print_error: None,
        printer_job_id: None,
        artifact_id: None,
        subtask_id: None,
        gcode_state: None,
        percent: None,
        speed_level: None,
        remaining_time_minutes: None,
        current_layer: None,
        total_layers: None,
        gcode_file: None,
        subtask_name: None,
        hms: None,
        diagnostics: Vec::new(),
        observed_at: "2026-07-10T00:00:00Z".to_owned(),
        printer_materials_json: String::new(),
    }
}
