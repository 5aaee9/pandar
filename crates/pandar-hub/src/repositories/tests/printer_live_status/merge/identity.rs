use super::*;

fn boundary_with_identities(
    source: &PrinterLiveStatus,
    report: &PrinterLiveStatusPatch,
) -> PrinterLiveStatus {
    let mut expected = boundary_state(source, source.task_generation + 1);
    expected.task_id.clone_from(&report.task_id);
    expected.subtask_id.clone_from(&report.subtask_id);
    expected.gcode_file.clone_from(&report.gcode_file);
    expected
}

fn identity_source(
    task: Option<&str>,
    subtask: Option<&str>,
    file: Option<&str>,
) -> PrinterLiveStatus {
    let mut source = stored();
    source.task_id = task.map(str::to_owned);
    source.subtask_id = subtask.map(str::to_owned);
    source.gcode_file = file.map(str::to_owned);
    source
}

fn ambiguity_with_identities(
    source: &PrinterLiveStatus,
    report: &PrinterLiveStatusPatch,
) -> PrinterLiveStatus {
    let mut expected = ambiguous_state(source);
    if report.task_id.is_some() {
        expected.task_id.clone_from(&report.task_id);
    }
    if report.subtask_id.is_some() {
        expected.subtask_id.clone_from(&report.subtask_id);
    }
    if report.gcode_file.is_some() {
        expected.gcode_file.clone_from(&report.gcode_file);
    }
    expected
}

#[test]
fn every_common_slot_conflict_is_a_single_fail_closed_boundary() {
    let source = stored();
    let reports = [
        (
            "task conflict",
            PrinterLiveStatusPatch {
                task_id: Some("task-b".to_owned()),
                ..patch()
            },
        ),
        (
            "subtask conflict",
            PrinterLiveStatusPatch {
                subtask_id: Some("subtask-b".to_owned()),
                ..patch()
            },
        ),
        (
            "file conflict",
            PrinterLiveStatusPatch {
                gcode_file: Some("/data/Metadata/plate-b.gcode".to_owned()),
                ..patch()
            },
        ),
        (
            "equal task and conflicting subtask",
            PrinterLiveStatusPatch {
                task_id: source.task_id.clone(),
                subtask_id: Some("subtask-b".to_owned()),
                ..patch()
            },
        ),
        (
            "equal task and conflicting file",
            PrinterLiveStatusPatch {
                task_id: source.task_id.clone(),
                gcode_file: Some("/data/Metadata/plate-b.gcode".to_owned()),
                ..patch()
            },
        ),
        (
            "equal subtask and conflicting task",
            PrinterLiveStatusPatch {
                task_id: Some("task-b".to_owned()),
                subtask_id: source.subtask_id.clone(),
                ..patch()
            },
        ),
        (
            "equal subtask and conflicting file",
            PrinterLiveStatusPatch {
                subtask_id: source.subtask_id.clone(),
                gcode_file: Some("/data/Metadata/plate-b.gcode".to_owned()),
                ..patch()
            },
        ),
        (
            "equal file and conflicting task",
            PrinterLiveStatusPatch {
                task_id: Some("task-b".to_owned()),
                gcode_file: source.gcode_file.clone(),
                ..patch()
            },
        ),
        (
            "equal file and conflicting subtask",
            PrinterLiveStatusPatch {
                subtask_id: Some("subtask-b".to_owned()),
                gcode_file: source.gcode_file.clone(),
                ..patch()
            },
        ),
        (
            "multiple conflicts",
            PrinterLiveStatusPatch {
                task_id: Some("task-b".to_owned()),
                subtask_id: Some("subtask-b".to_owned()),
                gcode_file: Some("/data/Metadata/plate-b.gcode".to_owned()),
                ..patch()
            },
        ),
    ];

    for (name, report) in reports {
        let expected = boundary_with_identities(&source, &report);
        assert_merge(name, &source, &report, SESSION_A, expected, true);
    }
}

#[test]
fn equality_partial_and_first_enrichment_preserve_the_task() {
    let mut enrichment_source = stored();
    enrichment_source.gcode_file = None;
    let enrichment = PrinterLiveStatusPatch {
        task_id: enrichment_source.task_id.clone(),
        gcode_file: Some("/data/Metadata/plate-a.gcode".to_owned()),
        ..patch()
    };
    let mut enrichment_expected = enrichment_source.clone();
    enrichment_expected
        .gcode_file
        .clone_from(&enrichment.gcode_file);
    assert_merge(
        "equal task plus file enrichment",
        &enrichment_source,
        &enrichment,
        SESSION_A,
        enrichment_expected,
        true,
    );

    let source = stored();
    let report = PrinterLiveStatusPatch {
        progress_percent: Some(43),
        ..patch()
    };
    let mut expected = source.clone();
    expected.progress_percent = Some(43);
    assert_merge(
        "missing identity is partial",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let first_source = identity_source(None, None, None);
    let report = PrinterLiveStatusPatch {
        task_id: Some("task-a".to_owned()),
        ..patch()
    };
    let mut expected = first_source.clone();
    expected.task_id.clone_from(&report.task_id);
    assert_merge(
        "first trusted identity enriches",
        &first_source,
        &report,
        SESSION_A,
        expected,
        true,
    );
}

#[test]
fn every_no_common_slot_shape_invalidates_only_recovery_context() {
    let cases = [
        (
            "task to subtask",
            identity_source(Some("t"), None, None),
            patch_with(None, Some("s"), None),
        ),
        (
            "task to file",
            identity_source(Some("t"), None, None),
            patch_with(None, None, Some("f")),
        ),
        (
            "subtask to task",
            identity_source(None, Some("s"), None),
            patch_with(Some("t"), None, None),
        ),
        (
            "subtask to file",
            identity_source(None, Some("s"), None),
            patch_with(None, None, Some("f")),
        ),
        (
            "file to task",
            identity_source(None, None, Some("f")),
            patch_with(Some("t"), None, None),
        ),
        (
            "file to subtask",
            identity_source(None, None, Some("f")),
            patch_with(None, Some("s"), None),
        ),
        (
            "task and subtask to file",
            identity_source(Some("t"), Some("s"), None),
            patch_with(None, None, Some("f")),
        ),
        (
            "task to subtask and file",
            identity_source(Some("t"), None, None),
            patch_with(None, Some("s"), Some("f")),
        ),
        (
            "equal literal in different namespaces",
            identity_source(Some("same"), None, None),
            patch_with(None, Some("same"), None),
        ),
        (
            "task sentinel plus disjoint subtask",
            identity_source(Some("t"), None, None),
            patch_with(Some("0"), Some("s"), None),
        ),
    ];

    for (name, source, report) in cases {
        let expected = ambiguity_with_identities(&source, &report);
        assert_merge(name, &source, &report, SESSION_A, expected, true);
    }
}

#[test]
fn blank_and_sentinel_identities_are_untrusted_but_file_zero_is_trusted() {
    let source = identity_source(Some("0"), Some("  "), None);
    let report = patch_with(Some("0"), Some("0"), None);
    let mut expected = source.clone();
    expected.task_id.clone_from(&report.task_id);
    expected.subtask_id.clone_from(&report.subtask_id);
    assert_merge(
        "task and subtask sentinels do not create a boundary",
        &source,
        &report,
        SESSION_A,
        expected,
        true,
    );

    let file_source = identity_source(None, None, Some("0"));
    let file_report = patch_with(None, None, Some("different.gcode"));
    let expected = boundary_with_identities(&file_source, &file_report);
    assert_merge(
        "file zero remains trusted identity",
        &file_source,
        &file_report,
        SESSION_A,
        expected,
        true,
    );
}

fn patch_with(
    task: Option<&str>,
    subtask: Option<&str>,
    file: Option<&str>,
) -> PrinterLiveStatusPatch {
    PrinterLiveStatusPatch {
        task_id: task.map(str::to_owned),
        subtask_id: subtask.map(str::to_owned),
        gcode_file: file.map(str::to_owned),
        ..patch()
    }
}
