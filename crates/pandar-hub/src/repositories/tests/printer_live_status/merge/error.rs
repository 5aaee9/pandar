use super::*;

fn set_marker(state: &mut PrinterLiveStatus, task_generation: u64, session_id: &str) {
    state.error_task_generation = Some(task_generation);
    state.error_session_id = Some(session_id.to_owned());
    state.error_received_at = Some(RECEIVED_AT.to_owned());
}

#[test]
fn positive_clear_same_positive_is_aba_safe() {
    let mut source = stored();
    source.print_error = Some(0);
    source.error_task_generation = None;
    source.error_session_id = None;
    source.error_received_at = None;
    let report = PrinterLiveStatusPatch {
        print_error: Some(ERROR),
        printer_job_id: Some("first-job".to_owned()),
        job_attr: Some(0),
        ..patch()
    };
    let mut expected = source.clone();
    expected.error_generation = 12;
    expected.print_error = Some(ERROR);
    expected.printer_job_id = Some("first-job".to_owned());
    expected.job_attr = Some(0);
    set_marker(&mut expected, 7, SESSION_A);
    let positive = assert_merge(
        "non-positive to positive",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(0),
        ..patch()
    };
    let mut expected = positive.clone();
    expected.print_error = Some(0);
    expected.error_task_generation = None;
    expected.error_session_id = None;
    expected.error_received_at = None;
    let cleared = assert_merge(
        "positive clear does not consume generation",
        &positive,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(ERROR),
        ..patch()
    };
    let mut expected = cleared.clone();
    expected.error_generation = 13;
    expected.print_error = Some(ERROR);
    expected.printer_job_id = None;
    expected.job_attr = None;
    set_marker(&mut expected, 7, SESSION_A);
    assert_merge(
        "same positive after clear",
        &cleared,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

#[test]
fn error_code_task_and_job_changes_advance_once() {
    let source = stored();
    let report = PrinterLiveStatusPatch {
        print_error: Some(17),
        ..patch()
    };
    let mut expected = source.clone();
    expected.error_generation = 12;
    expected.print_error = Some(17);
    expected.error_received_at = Some(RECEIVED_AT.to_owned());
    assert_merge(
        "different positive code",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(17),
        printer_job_id: Some("printer-job-b".to_owned()),
        ..patch()
    };
    let mut expected = source.clone();
    expected.error_generation = 12;
    expected.print_error = Some(17);
    expected.printer_job_id = Some("printer-job-b".to_owned());
    expected.error_received_at = Some(RECEIVED_AT.to_owned());
    assert_merge(
        "code and job change advance once",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        task_id: Some("task-b".to_owned()),
        print_error: Some(ERROR),
        ..patch()
    };
    let mut expected = boundary_state(&source, 8);
    expected.task_id = Some("task-b".to_owned());
    expected.error_generation = 12;
    expected.print_error = Some(ERROR);
    set_marker(&mut expected, 8, SESSION_A);
    assert_merge(
        "task boundary and positive advance once",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        task_id: Some("task-b".to_owned()),
        ..patch()
    };
    let mut expected = boundary_state(&source, 8);
    expected.task_id = Some("task-b".to_owned());
    assert_merge(
        "task boundary with absent error clears without an occurrence",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

#[test]
fn job_id_only_changes_advance_positive_occurrence_without_refreshing_marker() {
    let source = stored();
    for (name, job_id) in [
        ("different job", "printer-job-b"),
        ("explicit empty job", ""),
    ] {
        let report = PrinterLiveStatusPatch {
            printer_job_id: Some(job_id.to_owned()),
            ..patch()
        };
        let mut expected = source.clone();
        expected.error_generation = 12;
        expected.printer_job_id = Some(job_id.to_owned());
        assert_merge(name, &source, &report, SESSION_A, expected, true);
    }

    let report = PrinterLiveStatusPatch {
        printer_job_id: source.printer_job_id.clone(),
        ..patch()
    };
    assert_merge(
        "same explicit job is not a new occurrence",
        &source,
        &report,
        SESSION_A,
        source.clone(),
        false,
    );
}

#[test]
fn missing_task_or_session_marker_requires_explicit_reobservation() {
    let mut missing = stored();
    missing.error_task_generation = None;
    missing.error_session_id = None;
    missing.error_received_at = None;
    let mut wrong_task = stored();
    wrong_task.error_task_generation = Some(6);
    let mut missing_time = stored();
    missing_time.error_received_at = None;

    for (name, source, session_id) in [
        ("missing marker", missing, SESSION_A),
        ("different task marker", wrong_task, SESSION_A),
        ("missing receive time", missing_time, SESSION_A),
        ("different session marker", stored(), SESSION_B),
    ] {
        let report = PrinterLiveStatusPatch {
            print_error: Some(ERROR),
            ..patch()
        };
        let mut expected = source.clone();
        expected.error_generation = 12;
        expected.job_attr = None;
        expected.printer_job_id = None;
        set_marker(&mut expected, 7, session_id);
        assert_merge(name, &source, &report, session_id, expected, true);
    }
}

#[test]
fn replacement_session_partial_and_explicit_reports_are_fail_closed() {
    let source = stored();
    assert_merge(
        "replacement partial cannot refresh marker",
        &source,
        &patch(),
        SESSION_B,
        source.clone(),
        false,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(ERROR),
        ..patch()
    };
    let mut expected = source.clone();
    expected.error_generation = 12;
    expected.job_attr = None;
    expected.printer_job_id = None;
    set_marker(&mut expected, 7, SESSION_B);
    assert_merge(
        "replacement explicit error cannot reuse omitted recovery fields",
        &source,
        &report,
        SESSION_B,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(ERROR),
        printer_job_id: Some(String::new()),
        job_attr: Some(0),
        ..patch()
    };
    let mut expected = source.clone();
    expected.error_generation = 12;
    expected.printer_job_id = Some(String::new());
    expected.job_attr = Some(0);
    set_marker(&mut expected, 7, SESSION_B);
    assert_merge(
        "replacement restores explicit empty job and zero attr",
        &source,
        &report,
        SESSION_B,
        expected,
        true,
    );
}

#[test]
fn zero_attr_clear_and_repeated_occurrence_obey_independent_presence_rules() {
    let source = stored();
    let report = PrinterLiveStatusPatch {
        job_attr: Some(0),
        ..patch()
    };
    let mut expected = source.clone();
    expected.job_attr = Some(0);
    assert_merge(
        "attr-only change does not advance error generation",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(0),
        printer_job_id: Some("printer-job-b".to_owned()),
        ..patch()
    };
    let mut expected = source.clone();
    expected.print_error = Some(0);
    expected.printer_job_id = Some("printer-job-b".to_owned());
    expected.error_task_generation = None;
    expected.error_session_id = None;
    expected.error_received_at = None;
    assert_merge(
        "explicit clear plus job change does not advance",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(ERROR),
        job_attr: Some(0),
        ..patch()
    };
    let mut expected = source.clone();
    expected.job_attr = Some(0);
    expected.error_received_at = Some(RECEIVED_AT.to_owned());
    assert_merge(
        "same positive plus attr change does not advance",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let report = PrinterLiveStatusPatch {
        print_error: Some(ERROR),
        printer_job_id: source.printer_job_id.clone(),
        ..patch()
    };
    let mut expected = source.clone();
    expected.error_received_at = Some(RECEIVED_AT.to_owned());
    assert_merge(
        "repeated occurrence only refreshes private marker",
        &source,
        &report,
        SESSION_A,
        expected,
        false,
    );
}

#[test]
fn ambiguous_positive_report_restores_only_current_recovery_fields() {
    let mut source = stored();
    source.task_id = None;
    source.gcode_file = None;

    for (name, job_id, job_attr) in [
        ("omitted recovery", None, None),
        ("explicit recovery", Some(String::new()), Some(0)),
    ] {
        let report = PrinterLiveStatusPatch {
            task_id: Some("task-b".to_owned()),
            print_error: Some(ERROR),
            printer_job_id: job_id.clone(),
            job_attr,
            ..patch()
        };
        let mut expected = ambiguous_state(&source);
        expected.task_id = Some("task-b".to_owned());
        expected.error_generation = 12;
        expected.printer_job_id = job_id;
        expected.job_attr = job_attr;
        set_marker(&mut expected, 7, SESSION_A);
        assert_merge(name, &source, &report, SESSION_A, expected, true);
    }
}
