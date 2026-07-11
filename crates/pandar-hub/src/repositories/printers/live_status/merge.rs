use super::{PrinterLiveStatus, PrinterLiveStatusPatch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MergedPrinterLiveStatus {
    pub(crate) state: PrinterLiveStatus,
    pub(crate) live_status_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativePrintState {
    Live,
    Terminal,
    Idle,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
enum IdentitySlot {
    Task,
    Subtask,
    GcodeFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityRelation {
    Continuous,
    Boundary,
    Ambiguous,
}

const IDENTITY_SLOTS: [IdentitySlot; 3] = [
    IdentitySlot::Task,
    IdentitySlot::Subtask,
    IdentitySlot::GcodeFile,
];

pub(crate) fn merge_live_report(
    stored: &PrinterLiveStatus,
    patch: &PrinterLiveStatusPatch,
    session_id: &str,
    received_at: &str,
) -> MergedPrinterLiveStatus {
    let mut state = stored.clone();
    if classify(patch.gcode_state.as_deref()) == NativePrintState::Idle {
        clear_task_state(&mut state);
        state.gcode_state = Some("IDLE".to_owned());
        if let Some(hms) = patch.hms.as_ref() {
            state.hms.clone_from(hms);
        }
        return merged(stored, state);
    }

    let relation = identity_relation(stored, patch);
    let incoming_state = classify(patch.gcode_state.as_deref());
    let state_boundary = matches!(
        classify(stored.gcode_state.as_deref()),
        NativePrintState::Idle | NativePrintState::Terminal
    ) && incoming_state == NativePrintState::Live;
    let has_initial_evidence = has_incoming_trusted_identity(patch)
        || matches!(
            incoming_state,
            NativePrintState::Live | NativePrintState::Terminal
        );

    let mut cleared_for_boundary = false;
    if state.task_generation == 0 {
        if has_initial_evidence {
            state.task_generation = 1;
        }
    } else if relation == IdentityRelation::Boundary || state_boundary {
        state.task_generation += 1;
        clear_task_state(&mut state);
        cleared_for_boundary = true;
    }
    if !cleared_for_boundary && relation == IdentityRelation::Ambiguous {
        invalidate_recovery(&mut state);
    }

    apply_display_patch(&mut state, patch);
    merge_error_and_recovery(&mut state, stored, patch, session_id, received_at);
    merged(stored, state)
}

fn classify(value: Option<&str>) -> NativePrintState {
    match value {
        Some("PREPARE" | "SLICING" | "RUNNING" | "PAUSE") => NativePrintState::Live,
        Some("FINISH" | "FAILED") => NativePrintState::Terminal,
        Some("IDLE") => NativePrintState::Idle,
        Some(_) | None => NativePrintState::Unknown,
    }
}

fn identity_relation(
    stored: &PrinterLiveStatus,
    patch: &PrinterLiveStatusPatch,
) -> IdentityRelation {
    let mut stored_has_identity = false;
    let mut incoming_has_identity = false;
    let mut has_common_slot = false;
    let mut has_conflict = false;

    for slot in IDENTITY_SLOTS {
        let stored_value = trusted_stored_identity(stored, slot);
        let incoming_value = trusted_incoming_identity(patch, slot);
        stored_has_identity |= stored_value.is_some();
        incoming_has_identity |= incoming_value.is_some();
        if let (Some(stored_value), Some(incoming_value)) = (stored_value, incoming_value) {
            has_common_slot = true;
            has_conflict |= stored_value != incoming_value;
        }
    }

    if has_conflict {
        IdentityRelation::Boundary
    } else if stored_has_identity && incoming_has_identity && !has_common_slot {
        IdentityRelation::Ambiguous
    } else {
        IdentityRelation::Continuous
    }
}

fn has_incoming_trusted_identity(patch: &PrinterLiveStatusPatch) -> bool {
    IDENTITY_SLOTS
        .into_iter()
        .any(|slot| trusted_incoming_identity(patch, slot).is_some())
}

fn trusted_stored_identity(state: &PrinterLiveStatus, slot: IdentitySlot) -> Option<&str> {
    trusted_identity(
        match slot {
            IdentitySlot::Task => state.task_id.as_deref(),
            IdentitySlot::Subtask => state.subtask_id.as_deref(),
            IdentitySlot::GcodeFile => state.gcode_file.as_deref(),
        },
        slot,
    )
}

fn trusted_incoming_identity(patch: &PrinterLiveStatusPatch, slot: IdentitySlot) -> Option<&str> {
    trusted_identity(
        match slot {
            IdentitySlot::Task => patch.task_id.as_deref(),
            IdentitySlot::Subtask => patch.subtask_id.as_deref(),
            IdentitySlot::GcodeFile => patch.gcode_file.as_deref(),
        },
        slot,
    )
}

fn trusted_identity(value: Option<&str>, slot: IdentitySlot) -> Option<&str> {
    value.map(str::trim).filter(|value| {
        !value.is_empty() && (matches!(slot, IdentitySlot::GcodeFile) || *value != "0")
    })
}

fn apply_display_patch(state: &mut PrinterLiveStatus, patch: &PrinterLiveStatusPatch) {
    apply_option(&mut state.task_id, &patch.task_id);
    apply_option(&mut state.subtask_id, &patch.subtask_id);
    apply_copy(&mut state.progress_percent, patch.progress_percent);
    apply_copy(
        &mut state.remaining_time_minutes,
        patch.remaining_time_minutes,
    );
    apply_copy(&mut state.current_layer, patch.current_layer);
    apply_copy(&mut state.total_layers, patch.total_layers);
    apply_option(&mut state.gcode_file, &patch.gcode_file);
    apply_option(&mut state.subtask_name, &patch.subtask_name);
    apply_option(&mut state.gcode_state, &patch.gcode_state);
    if let Some(hms) = patch.hms.as_ref() {
        state.hms.clone_from(hms);
    }
}

fn merge_error_and_recovery(
    state: &mut PrinterLiveStatus,
    stored: &PrinterLiveStatus,
    patch: &PrinterLiveStatusPatch,
    session_id: &str,
    received_at: &str,
) {
    let task_changed = state.task_generation != stored.task_generation;
    let job_changed = patch
        .printer_job_id
        .as_ref()
        .is_some_and(|job_id| stored.printer_job_id.as_ref() != Some(job_id));

    match patch.print_error {
        Some(0) => {
            state.print_error = Some(0);
            clear_error_marker(state);
        }
        Some(error) => {
            let marker_valid = error_marker_is_valid(state, session_id);
            let occurrence_changed =
                stored.print_error != Some(error) || task_changed || job_changed || !marker_valid;
            if !marker_valid {
                state.printer_job_id = None;
                state.job_attr = None;
            }
            if occurrence_changed {
                state.error_generation += 1;
            }
            state.print_error = Some(error);
            state.error_task_generation = Some(state.task_generation);
            state.error_session_id = Some(session_id.to_owned());
            state.error_received_at = Some(received_at.to_owned());
        }
        None => {
            if state.print_error.is_some_and(|error| error > 0) && (task_changed || job_changed) {
                state.error_generation += 1;
            }
        }
    }

    apply_option(&mut state.printer_job_id, &patch.printer_job_id);
    apply_copy(&mut state.job_attr, patch.job_attr);
}

fn error_marker_is_valid(state: &PrinterLiveStatus, session_id: &str) -> bool {
    state.error_task_generation == Some(state.task_generation)
        && state.error_session_id.as_deref() == Some(session_id)
        && state.error_received_at.is_some()
}

fn clear_task_state(state: &mut PrinterLiveStatus) {
    state.task_id = None;
    state.subtask_id = None;
    state.progress_percent = None;
    state.remaining_time_minutes = None;
    state.current_layer = None;
    state.total_layers = None;
    state.gcode_file = None;
    state.subtask_name = None;
    state.print_error = None;
    state.printer_job_id = None;
    state.job_attr = None;
    clear_error_marker(state);
}

fn invalidate_recovery(state: &mut PrinterLiveStatus) {
    state.printer_job_id = None;
    state.job_attr = None;
    clear_error_marker(state);
}

fn clear_error_marker(state: &mut PrinterLiveStatus) {
    state.error_task_generation = None;
    state.error_session_id = None;
    state.error_received_at = None;
}

fn apply_option(target: &mut Option<String>, patch: &Option<String>) {
    if let Some(value) = patch.as_ref() {
        *target = Some(value.clone());
    }
}

fn apply_copy<T: Copy>(target: &mut Option<T>, patch: Option<T>) {
    if let Some(value) = patch {
        *target = Some(value);
    }
}

fn merged(stored: &PrinterLiveStatus, state: PrinterLiveStatus) -> MergedPrinterLiveStatus {
    let mut old_public_state = stored.clone();
    old_public_state.job_attr = public_job_state(old_public_state.job_attr);
    clear_error_marker(&mut old_public_state);
    let mut new_public_state = state.clone();
    new_public_state.job_attr = public_job_state(new_public_state.job_attr);
    clear_error_marker(&mut new_public_state);
    MergedPrinterLiveStatus {
        live_status_changed: old_public_state != new_public_state,
        state,
    }
}

fn public_job_state(job_attr: Option<u32>) -> Option<u32> {
    job_attr.map(|value| (value >> 4) & 0x0f)
}
