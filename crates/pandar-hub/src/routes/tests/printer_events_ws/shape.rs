use super::*;

#[test]
fn printer_event_rolling_decode_accepts_legacy_and_old_shape_ignores_enrichment() {
    let legacy = serde_json::json!({
        "type": "printer_snapshot",
        "printer": {
            "id": "printer-1",
            "tenant_id": "00000000-0000-4000-8000-000000000001",
            "agent_id": "00000000-0000-4000-8000-000000000002",
            "serial_number": "SN-1",
            "name": "Printer",
            "model": null,
            "status": "IDLE",
            "last_seen_at": "2026-07-10T00:00:00Z",
            "created_at": "2026-07-10T00:00:00Z",
            "nozzle_temperatures": [],
            "active_nozzle": null,
            "bed_temperature_celsius": null,
            "bed_target_temperature_celsius": null,
            "chamber_temperature_celsius": null,
            "chamber_light_on": null,
            "materials": null
        }
    });
    let decoded: crate::printer_events::PrinterEvent =
        serde_json::from_value(legacy).expect("legacy snapshot should decode");
    let crate::printer_events::PrinterEvent::PrinterSnapshot { printer } = decoded else {
        panic!("expected legacy printer snapshot")
    };
    assert_eq!(printer.state_revision, None);
    assert_eq!(printer.print, None);

    let enriched = serde_json::json!({
        "type": "printer_snapshot",
        "printer": {
            "id": "printer-1",
            "status": "RUNNING",
            "state_revision": 9,
            "print": {
                "task_generation": 2,
                "error_generation": 3,
                "hms": [],
                "job_state": null,
                "gcode_state": "RUNNING",
                "task_id": null,
                "subtask_id": null,
                "progress_percent": null,
                "remaining_time_minutes": null,
                "current_layer": null,
                "total_layers": null,
                "gcode_file": null,
                "subtask_name": null,
                "print_error": null,
                "printer_job_id": null
            }
        }
    });
    let old: OldShapePrinterEvent =
        serde_json::from_value(enriched).expect("old shape should ignore additive fields");
    let OldShapePrinterEvent::PrinterSnapshot { printer } = old;
    assert_eq!(printer.id, "printer-1");
    assert_eq!(printer.status, "RUNNING");
}
