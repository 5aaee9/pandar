use crate::repositories::{
    PrinterHms, PrinterLiveStatus,
    printers::{PrinterLiveStatusPatch, merge_live_report},
};

const SESSION_A: &str = "session-a";
const SESSION_B: &str = "session-b";
const RECEIVED_AT: &str = "2026-07-10T12:00:00Z";
const ERROR: u32 = 83_918_929;

mod error;
mod identity;
mod state;

fn stored() -> PrinterLiveStatus {
    PrinterLiveStatus {
        task_generation: 7,
        error_generation: 11,
        job_attr: Some(0x21),
        error_task_generation: Some(7),
        error_session_id: Some(SESSION_A.to_owned()),
        error_received_at: Some("2026-07-10T11:59:00Z".to_owned()),
        gcode_state: Some("RUNNING".to_owned()),
        task_id: Some("task-a".to_owned()),
        subtask_id: Some("subtask-a".to_owned()),
        progress_percent: Some(42),
        remaining_time_minutes: Some(11),
        current_layer: Some(2),
        total_layers: Some(128),
        gcode_file: Some("/data/Metadata/plate-a.gcode".to_owned()),
        subtask_name: Some("Plate A".to_owned()),
        print_error: Some(ERROR),
        printer_job_id: Some("printer-job-a".to_owned()),
        hms: vec![PrinterHms { attr: 1, code: 2 }],
    }
}

fn patch() -> PrinterLiveStatusPatch {
    PrinterLiveStatusPatch::default()
}

fn boundary_state(source: &PrinterLiveStatus, generation: u64) -> PrinterLiveStatus {
    PrinterLiveStatus {
        task_generation: generation,
        error_generation: source.error_generation,
        job_attr: None,
        error_task_generation: None,
        error_session_id: None,
        error_received_at: None,
        gcode_state: source.gcode_state.clone(),
        task_id: None,
        subtask_id: None,
        progress_percent: None,
        remaining_time_minutes: None,
        current_layer: None,
        total_layers: None,
        gcode_file: None,
        subtask_name: None,
        print_error: None,
        printer_job_id: None,
        hms: source.hms.clone(),
    }
}

fn ambiguous_state(source: &PrinterLiveStatus) -> PrinterLiveStatus {
    let mut expected = source.clone();
    expected.job_attr = None;
    expected.printer_job_id = None;
    expected.error_task_generation = None;
    expected.error_session_id = None;
    expected.error_received_at = None;
    expected
}

fn assert_merge(
    name: &str,
    source: &PrinterLiveStatus,
    report: &PrinterLiveStatusPatch,
    session_id: &str,
    expected: PrinterLiveStatus,
    changed: bool,
) -> PrinterLiveStatus {
    let merged = merge_live_report(source, report, session_id, RECEIVED_AT);
    assert_eq!(merged.state, expected, "{name}");
    assert_eq!(merged.live_status_changed, changed, "{name}");
    merged.state
}
