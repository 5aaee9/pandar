use super::*;

#[test]
fn interpretation_reports_typed_section_decode_failures_in_stable_order() {
    let interpreted = interpret_report(&endpoint(), serde_json::json!({ "print": [] }));

    assert!(interpreted.print.is_none());
    assert!(interpreted.snapshot.is_none());
    assert!(interpreted.materials.is_none());
    assert_eq!(
        interpreted
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.section)
            .collect::<Vec<_>>(),
        vec![
            report::MachineReportSection::Print,
            report::MachineReportSection::Snapshot,
        ]
    );
    assert!(interpreted.diagnostics.iter().all(|diagnostic| {
        let error = format!("{diagnostic:#}");
        error.contains("Machine report section") && error.contains(&diagnostic.source.to_string())
    }));
}

#[test]
fn interpretation_preserves_snapshot_when_materials_fail_to_decode() {
    let interpreted = interpret_report(
        &endpoint(),
        serde_json::json!({
            "print": {
                "gcode_state": "RUNNING",
                "ams": { "ams": "invalid" }
            }
        }),
    );

    assert_eq!(
        interpreted.snapshot.unwrap().state.as_deref(),
        Some("RUNNING")
    );
    assert!(interpreted.materials.is_none());
    assert_eq!(interpreted.diagnostics.len(), 1);
    assert_eq!(
        interpreted.diagnostics[0].section,
        report::MachineReportSection::Materials
    );
}

#[test]
fn interpretation_classifies_print_telemetry_and_intrinsic_authority() {
    for (value, expected) in [
        (
            serde_json::json!({
                "info": { "command": "get_version", "module": [] }
            }),
            PrintTelemetryClass::None,
        ),
        (
            serde_json::json!({
                "print": {
                    "command": "push_status",
                    "msg": 1,
                    "cfg": "force-upgrade",
                    "upgrade_state": { "status": "DOWNLOADING" }
                }
            }),
            PrintTelemetryClass::ProtocolOnly,
        ),
        (
            serde_json::json!({
                "print": {
                    "msg": 1,
                    "upgrade_state": { "status": "DOWNLOADING" },
                    "gcode_state": "RUNNING"
                }
            }),
            PrintTelemetryClass::Operational,
        ),
    ] {
        assert_eq!(interpret_report(&endpoint(), value).facts.print, expected);
    }

    let interpreted = interpret_report(
        &endpoint(),
        serde_json::json!({
            "print": { "command": "push_status", "msg": 0 }
        }),
    );
    assert_eq!(
        interpreted.facts.authority,
        report::SnapshotAuthority::FullPushStatus
    );
}
