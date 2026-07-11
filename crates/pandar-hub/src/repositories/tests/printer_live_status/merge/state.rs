use super::*;

#[test]
fn every_inactive_or_terminal_to_native_live_transition_is_one_boundary() {
    for old_state in ["IDLE", "FINISH", "FAILED"] {
        for new_state in ["PREPARE", "SLICING", "RUNNING", "PAUSE"] {
            let mut source = stored();
            source.gcode_state = Some(old_state.to_owned());
            let report = PrinterLiveStatusPatch {
                gcode_state: Some(new_state.to_owned()),
                ..patch()
            };
            let mut expected = boundary_state(&source, 8);
            expected.gcode_state = Some(new_state.to_owned());
            assert_merge(
                &format!("{old_state} to {new_state}"),
                &source,
                &report,
                SESSION_A,
                expected,
                true,
            );
        }
    }

    let mut source = stored();
    source.gcode_state = Some("FINISH".to_owned());
    let report = PrinterLiveStatusPatch {
        gcode_state: Some("RUNNING".to_owned()),
        task_id: Some("task-b".to_owned()),
        ..patch()
    };
    let mut expected = boundary_state(&source, 8);
    expected.gcode_state = Some("RUNNING".to_owned());
    expected.task_id = Some("task-b".to_owned());
    assert_merge(
        "state and identity boundary increments once",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

#[test]
fn aliases_unknown_and_live_to_live_do_not_create_native_boundaries() {
    for (old_state, new_state) in [
        ("FINISH", "PRINTING"),
        ("FAILED", "PAUSED"),
        ("UNKNOWN", "RUNNING"),
        ("RUNNING", "PAUSE"),
    ] {
        let mut source = stored();
        source.gcode_state = Some(old_state.to_owned());
        let report = PrinterLiveStatusPatch {
            gcode_state: Some(new_state.to_owned()),
            ..patch()
        };
        let mut expected = source.clone();
        expected.gcode_state = Some(new_state.to_owned());
        assert_merge(
            &format!("{old_state} to {new_state}"),
            &source,
            &report,
            SESSION_A,
            expected,
            true,
        );
    }
}

#[test]
fn idle_has_highest_precedence_and_terminal_frames_retain_final_fields() {
    let source = stored();
    let report = PrinterLiveStatusPatch {
        gcode_state: Some("IDLE".to_owned()),
        task_id: Some("stale-task".to_owned()),
        subtask_id: Some("stale-subtask".to_owned()),
        progress_percent: Some(99),
        remaining_time_minutes: Some(99),
        current_layer: Some(99),
        total_layers: Some(99),
        gcode_file: Some("stale.gcode".to_owned()),
        subtask_name: Some("Stale".to_owned()),
        print_error: Some(ERROR),
        printer_job_id: Some("stale-job".to_owned()),
        job_attr: Some(0x31),
        hms: Some(vec![PrinterHms { attr: 3, code: 4 }]),
        observed_at: String::new(),
    };
    let mut expected = boundary_state(&source, 7);
    expected.gcode_state = Some("IDLE".to_owned());
    expected.hms = report.hms.clone().unwrap();
    assert_merge(
        "IDLE ignores stale task data",
        &source,
        &report,
        SESSION_B,
        expected,
        true,
    );

    for terminal in ["FINISH", "FAILED"] {
        let report = PrinterLiveStatusPatch {
            gcode_state: Some(terminal.to_owned()),
            ..patch()
        };
        let mut expected = source.clone();
        expected.gcode_state = Some(terminal.to_owned());
        assert_merge(
            &format!("{terminal} retains task fields"),
            &source,
            &report,
            SESSION_A,
            expected,
            true,
        );
    }
}

#[test]
fn explicit_zero_empty_and_absent_fields_preserve_presence_semantics() {
    let source = stored();
    let report = PrinterLiveStatusPatch {
        task_id: source.task_id.clone(),
        progress_percent: Some(0),
        remaining_time_minutes: Some(0),
        current_layer: Some(0),
        total_layers: Some(0),
        print_error: Some(0),
        printer_job_id: Some(String::new()),
        job_attr: Some(0),
        hms: Some(Vec::new()),
        ..patch()
    };
    let mut expected = source.clone();
    expected.progress_percent = Some(0);
    expected.remaining_time_minutes = Some(0);
    expected.current_layer = Some(0);
    expected.total_layers = Some(0);
    expected.print_error = Some(0);
    expected.printer_job_id = Some(String::new());
    expected.job_attr = Some(0);
    expected.hms.clear();
    expected.error_task_generation = None;
    expected.error_session_id = None;
    expected.error_received_at = None;
    assert_merge(
        "explicit values overwrite",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    assert_merge(
        "absent fields preserve values and old-session marker",
        &source,
        &patch(),
        SESSION_B,
        source.clone(),
        false,
    );
}

#[test]
fn boundary_applies_only_current_frame_zero_and_empty_values() {
    let mut source = stored();
    source.gcode_state = Some("FINISH".to_owned());
    let report = PrinterLiveStatusPatch {
        gcode_state: Some("RUNNING".to_owned()),
        task_id: Some("task-b".to_owned()),
        progress_percent: Some(0),
        remaining_time_minutes: Some(0),
        current_layer: Some(0),
        total_layers: Some(0),
        print_error: Some(0),
        printer_job_id: Some(String::new()),
        job_attr: Some(0),
        ..patch()
    };
    let mut expected = boundary_state(&source, 8);
    expected.gcode_state = Some("RUNNING".to_owned());
    expected.task_id = Some("task-b".to_owned());
    expected.progress_percent = Some(0);
    expected.remaining_time_minutes = Some(0);
    expected.current_layer = Some(0);
    expected.total_layers = Some(0);
    expected.print_error = Some(0);
    expected.printer_job_id = Some(String::new());
    expected.job_attr = Some(0);
    assert_merge(
        "boundary restores explicit current-frame values",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

#[test]
fn generation_zero_initializes_only_from_reviewed_task_evidence() {
    let source = generation_zero_source();
    let report = PrinterLiveStatusPatch {
        task_id: Some("first-task".to_owned()),
        ..patch()
    };
    let mut expected = source.clone();
    expected.task_generation = 1;
    expected.task_id = Some("first-task".to_owned());
    assert_merge(
        "trusted identity initializes",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    for native in ["PREPARE", "SLICING", "RUNNING", "PAUSE", "FINISH", "FAILED"] {
        let source = generation_zero_source();
        let report = PrinterLiveStatusPatch {
            gcode_state: Some(native.to_owned()),
            ..patch()
        };
        let mut expected = source.clone();
        expected.task_generation = 1;
        expected.gcode_state = Some(native.to_owned());
        assert_merge(native, &source, &report, SESSION_A, expected, true);
    }

    for alias in ["PRINTING", "PAUSED", "UNKNOWN"] {
        let source = generation_zero_source();
        let report = PrinterLiveStatusPatch {
            gcode_state: Some(alias.to_owned()),
            ..patch()
        };
        let mut expected = source.clone();
        expected.gcode_state = Some(alias.to_owned());
        assert_merge(alias, &source, &report, SESSION_A, expected, true);
    }

    let source = generation_zero_source();
    assert_merge(
        "no evidence stays generation zero",
        &source,
        &patch(),
        SESSION_A,
        source.clone(),
        false,
    );

    let source = generation_zero_source();
    let report = PrinterLiveStatusPatch {
        gcode_state: Some("IDLE".to_owned()),
        task_id: Some("ignored-task".to_owned()),
        ..patch()
    };
    let mut expected = boundary_state(&source, 0);
    expected.gcode_state = Some("IDLE".to_owned());
    assert_merge(
        "IDLE never initializes generation",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

#[test]
fn externally_visible_hms_and_derived_job_state_drive_live_changes() {
    let source = stored();
    let report = PrinterLiveStatusPatch {
        hms: Some(vec![PrinterHms { attr: 3, code: 4 }]),
        ..patch()
    };
    let mut expected = source.clone();
    expected.hms = report.hms.clone().unwrap();
    assert_merge(
        "HMS is externally visible",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        job_attr: Some(0x22),
        ..patch()
    };
    let mut expected = source.clone();
    expected.job_attr = Some(0x22);
    assert_merge(
        "raw job attr change with same derived job state is private",
        &source,
        &report,
        SESSION_A,
        expected,
        false,
    );

    let report = PrinterLiveStatusPatch {
        job_attr: Some(0x11),
        ..patch()
    };
    let mut expected = source.clone();
    expected.job_attr = Some(0x11);
    assert_merge(
        "job attr changes derived job state",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

fn generation_zero_source() -> PrinterLiveStatus {
    let mut source = stored();
    source.task_generation = 0;
    source.task_id = None;
    source.subtask_id = None;
    source.gcode_file = None;
    source.gcode_state = None;
    source.print_error = None;
    source.error_task_generation = None;
    source.error_session_id = None;
    source.error_received_at = None;
    source
}
