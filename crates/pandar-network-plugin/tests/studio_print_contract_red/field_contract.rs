use crate::{
    field_cases::{Expectation, FIELD_CASES, FieldCase, QUEUE_PLATE_ID_FIELD, SLICER_UID_FIELD},
    harness::{ProbeEvidence, print_requests, run_probe},
};

fn selected_field_cases() -> Vec<FieldCase> {
    let capabilities = &pandar_studio_profile::abi_series(pandar_network_plugin::STUDIO_ABI_SERIES)
        .unwrap()
        .capabilities;
    let mut cases = FIELD_CASES.to_vec();
    if capabilities.print_slicer_uid {
        cases.push(SLICER_UID_FIELD);
    }
    if capabilities.print_queue_plate_id {
        cases.push(QUEUE_PLATE_ID_FIELD);
    }
    cases
}

fn multipart_value(request: &str, field: &str) -> Option<String> {
    let marker = format!(r#"name="{field}""#);
    let part = request.split_once(&marker)?.1;
    let value = part.split_once("\r\n\r\n")?.1;
    Some(value.split("\r\n").next()?.to_owned())
}

fn combined_evidence(evidence: &ProbeEvidence) -> String {
    format!(
        "{}\n{}\n{}",
        evidence.stdout,
        evidence.stderr,
        evidence.requests.join("\n")
    )
}

fn leakage_failures(evidence: &ProbeEvidence, field: &str) -> Vec<String> {
    let combined = combined_evidence(evidence);
    let mut forbidden = vec![
        evidence.artifact_path.as_str(),
        evidence.config_path.as_str(),
        "username-secret-sentinel",
        "password-secret-sentinel",
        "198.51.100.77",
        "/contract/private/ftp-folder",
        "ftp-object.3mf",
        "0123456789abcdef0123456789abcdef",
        "sdcard/contract.3mf",
        "/private/diagnostic-secret-token@198.51.100.91",
        "diagnostic-secret-token-198.51.100.91",
    ];
    if field == "extra_options" {
        forbidden.push(r#"{"future":true}"#);
    }
    forbidden
        .into_iter()
        .filter(|value| combined.contains(value))
        .map(|value| format!("{field}: secret or local-only value leaked: {value}"))
        .collect()
}

fn error_contract(evidence: &ProbeEvidence, field: &str, error: &str) -> Result<(), String> {
    if let Some(leak) = leakage_failures(evidence, field).into_iter().next() {
        return Err(leak);
    }
    if evidence.output["rc"] != serde_json::json!(-19) {
        return Err(format!(
            "{field}: expected -19, got {}",
            evidence.output["rc"]
        ));
    }
    if evidence.output["stages"] != serde_json::json!([7])
        || evidence.output["codes"] != serde_json::json!([-19])
    {
        return Err(format!(
            "{field}: expected one ERROR/-19 callback, got stages={} codes={}",
            evidence.output["stages"], evidence.output["codes"]
        ));
    }
    let expected = serde_json::json!({"error": error, "field": field}).to_string();
    if evidence.output["bodies"] != serde_json::json!([expected]) {
        return Err(format!(
            "{field}: wrong redacted callback body {}",
            evidence.output["bodies"]
        ));
    }
    if !print_requests(evidence).is_empty() {
        return Err(format!(
            "{field}: rejected value reached Hub print submission"
        ));
    }
    Ok(())
}

#[test]
fn all_selected_print_params_have_explicit_compiled_abi_dispositions() {
    let mut failures = Vec::new();
    for (field, disposition, expected) in selected_field_cases() {
        let evidence = run_probe("print", field);
        failures.extend(leakage_failures(&evidence, field));
        match expected {
            Expectation::Reject => {
                if let Err(error) = error_contract(&evidence, field, "invalid_print_param") {
                    failures.push(error);
                }
                continue;
            }
            Expectation::Unsupported => {
                if let Err(error) = error_contract(&evidence, field, "unsupported_print_param") {
                    failures.push(error);
                }
                continue;
            }
            _ => {}
        }

        if evidence.output["rc"] != serde_json::json!(0) {
            failures.push(format!(
                "{field} ({disposition}): expected admission success, got {}",
                evidence.output["rc"]
            ));
            continue;
        }
        let requests = print_requests(&evidence);
        if requests.len() != 1 {
            failures.push(format!(
                "{field} ({disposition}): expected one Hub print request, got {}",
                requests.len()
            ));
            continue;
        }
        let request = requests[0];
        match expected {
            Expectation::Wire(wire_field, value) => {
                if multipart_value(request, wire_field).as_deref() != Some(value) {
                    failures.push(format!(
                        "{field}: missing preserved multipart {wire_field}={value}"
                    ));
                }
                if field == "dev_id" && request.contains("studio-serial-1") {
                    failures.push(
                        "dev_id: Studio serial leaked instead of authorized Hub id".to_owned(),
                    );
                }
            }
            Expectation::WireJson(wire_field, value) => {
                let actual = multipart_value(request, wire_field)
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok());
                let value: serde_json::Value = serde_json::from_str(value).unwrap();
                if actual.as_ref() != Some(&value) {
                    failures.push(format!(
                        "{field}: missing typed JSON multipart {wire_field}; got {actual:?}"
                    ));
                }
            }
            Expectation::ArtifactSource => {
                if !request.contains("studio print contract artifact bytes") {
                    failures.push("filename: artifact bytes were not uploaded".to_owned());
                }
            }
            Expectation::ConfigSource => {
                if request.contains(&evidence.config_path)
                    || request.contains("contract-private-config.3mf")
                {
                    failures.push("config_filename: local config path leaked".to_owned());
                }
                if multipart_value(request, "config_plate_index").as_deref() != Some("7") {
                    failures
                        .push("config_filename: typed plate index was not preserved".to_owned());
                }
            }
            Expectation::ScrubValue(value) => {
                if combined_evidence(&evidence).contains(value) {
                    failures.push(format!("{field}: scrubbed value leaked"));
                }
            }
            Expectation::ScrubField(wire_field) => {
                if request.contains(&format!(r#"name="{wire_field}""#)) {
                    failures.push(format!("{field}: plugin-local field reached Hub"));
                }
            }
            Expectation::Admitted | Expectation::Reject | Expectation::Unsupported => {}
        }
    }
    assert!(
        failures.is_empty(),
        "compiled ABI field contract failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn typed_print_admission_rejects_invalid_values_before_http() {
    let cases = [
        ("invalid_plate_index", "plate_index", "invalid_print_param"),
        (
            "invalid_config_xml",
            "config_filename",
            "invalid_print_param",
        ),
        (
            "invalid_nozzle_mapping",
            "nozzle_mapping",
            "invalid_print_param",
        ),
        ("invalid_ams_mapping", "ams_mapping", "invalid_print_param"),
        (
            "invalid_ams_mapping2",
            "ams_mapping2",
            "invalid_print_param",
        ),
        (
            "invalid_ams_mapping_info",
            "ams_mapping_info",
            "invalid_print_param",
        ),
        (
            "invalid_nozzles_info",
            "nozzles_info",
            "invalid_print_param",
        ),
        (
            "whitespace_nozzle_mapping",
            "nozzle_mapping",
            "invalid_print_param",
        ),
        (
            "schema_nozzle_mapping",
            "nozzle_mapping",
            "invalid_print_param",
        ),
        ("schema_ams_mapping", "ams_mapping", "invalid_print_param"),
        ("schema_ams_mapping2", "ams_mapping2", "invalid_print_param"),
        (
            "schema_ams_mapping_info",
            "ams_mapping_info",
            "invalid_print_param",
        ),
        ("schema_nozzles_info", "nozzles_info", "invalid_print_param"),
        (
            "invalid_task_bed_type",
            "task_bed_type",
            "invalid_print_param",
        ),
        (
            "invalid_connection_type",
            "connection_type",
            "invalid_print_param",
        ),
        (
            "unsupported_print_type",
            "print_type",
            "unsupported_print_param",
        ),
        (
            "invalid_auto_bed_leveling",
            "auto_bed_leveling",
            "invalid_print_param",
        ),
        (
            "invalid_auto_flow_cali",
            "auto_flow_cali",
            "invalid_print_param",
        ),
        (
            "invalid_auto_offset_cali",
            "auto_offset_cali",
            "invalid_print_param",
        ),
        (
            "invalid_extruder_cali_manual_mode",
            "extruder_cali_manual_mode",
            "invalid_print_param",
        ),
    ];
    let failures = cases
        .into_iter()
        .filter_map(|(case, field, error)| {
            error_contract(&run_probe("print", case), field, error).err()
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "typed admission failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn pinned_empty_mapping_strings_become_typed_empty_arrays() {
    let evidence = run_probe("print", "empty_mapping_strings");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    let requests = print_requests(&evidence);
    assert_eq!(requests.len(), 1);
    for field in [
        "nozzle_mapping",
        "ams_mapping",
        "ams_mapping2",
        "ams_mapping_info",
        "nozzles_info",
    ] {
        assert_eq!(multipart_value(requests[0], field).as_deref(), Some("[]"));
    }
}

#[test]
fn pinned_empty_cloud_connection_type_is_canonicalized() {
    let evidence = run_probe("print", "empty_connection_type");
    assert_eq!(evidence.output["rc"], serde_json::json!(0));
    let requests = print_requests(&evidence);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        multipart_value(requests[0], "connection_type").as_deref(),
        Some("cloud")
    );
}
