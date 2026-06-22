use anyhow::Context;
use pandar_core::{JobId, PrintStatus};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::{
    entities::jobs,
    repositories::{JobWithArtifact, RepositoryResult},
};

use super::ApplyPrintReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PrintUpdate {
    pub(super) print_status: String,
    pub(super) printer_state: Option<String>,
    pub(super) progress_percent: Option<u8>,
    pub(super) remaining_time_minutes: Option<u32>,
    pub(super) current_layer: Option<u32>,
    pub(super) total_layers: Option<u32>,
    pub(super) active_file: Option<String>,
    pub(super) last_progress_percent: Option<u8>,
    pub(super) last_layer: Option<u32>,
    pub(super) print_error: Option<String>,
    pub(super) print_started_at: Option<String>,
    pub(super) print_finished_at: Option<String>,
    pub(super) print_updated_at: Option<String>,
}

pub(super) fn update_from_job(job: &JobWithArtifact) -> PrintUpdate {
    PrintUpdate {
        print_status: job.job.print.status.as_str().to_string(),
        printer_state: job.job.print.printer_state.clone(),
        progress_percent: job.job.print.progress_percent,
        remaining_time_minutes: job.job.print.remaining_time_minutes,
        current_layer: job.job.print.current_layer,
        total_layers: job.job.print.total_layers,
        active_file: job.job.print.active_file.clone(),
        last_progress_percent: job.job.print.last_progress_percent,
        last_layer: job.job.print.last_layer,
        print_error: job.job.print.error.clone(),
        print_started_at: job.job.print.started_at.clone(),
        print_finished_at: job.job.print.finished_at.clone(),
        print_updated_at: job.job.print.updated_at.clone(),
    }
}

pub(super) fn reconciled_update(current: &PrintUpdate, input: &ApplyPrintReport) -> PrintUpdate {
    let mut next = current.clone();
    let current_status = current.print_status.parse::<PrintStatus>().ok();
    let is_terminal = matches!(
        current_status,
        Some(PrintStatus::Completed | PrintStatus::Failed | PrintStatus::Cancelled)
    );
    let incoming_status = incoming_status(input.gcode_state.as_deref(), current_status.as_ref());

    if !is_terminal && let Some(status) = &incoming_status {
        next.print_status = status.as_str().to_string();
        if *status == PrintStatus::Running && next.print_started_at.is_none() {
            next.print_started_at = Some(input.observed_at.clone());
        }
        if is_terminal_status(status) && next.print_finished_at.is_none() {
            next.print_finished_at = Some(input.observed_at.clone());
        }
        if *status == PrintStatus::Failed {
            next.print_error =
                Some(terminal_error(input).unwrap_or_else(|| "print failed".to_string()));
        } else if *status == PrintStatus::Cancelled && next.print_error.is_none() {
            next.print_error =
                Some(terminal_error(input).unwrap_or_else(|| "print cancelled".to_string()));
        }
    }

    if input.gcode_state.is_some() {
        next.printer_state = input.gcode_state.clone();
    }
    if let Some(percent) = input.percent {
        next.progress_percent = Some(percent);
        next.last_progress_percent = Some(
            next.last_progress_percent
                .map_or(percent, |last| last.max(percent)),
        );
    }
    if let Some(minutes) = input.remaining_time_minutes {
        next.remaining_time_minutes = Some(minutes);
    }
    if let Some(layer) = input.current_layer {
        next.current_layer = Some(layer);
        next.last_layer = Some(next.last_layer.map_or(layer, |last| last.max(layer)));
    }
    if let Some(total_layers) = input.total_layers {
        next.total_layers = Some(total_layers);
    }
    if let Some(active_file) = input
        .gcode_file
        .as_ref()
        .or(input.subtask_name.as_ref())
        .filter(|value| !value.trim().is_empty())
    {
        next.active_file = Some(active_file.trim().to_string());
    }
    next.print_updated_at = Some(input.observed_at.clone());
    next
}

pub(super) fn is_terminal_status(status: &PrintStatus) -> bool {
    matches!(
        status,
        PrintStatus::Completed | PrintStatus::Failed | PrintStatus::Cancelled
    )
}

pub(super) async fn update_job_print<C>(
    connection: &C,
    job_id: &JobId,
    update: &PrintUpdate,
) -> RepositoryResult<bool>
where
    C: ConnectionTrait,
{
    let result = jobs::Entity::update_many()
        .set(jobs::ActiveModel {
            print_status: Set(update.print_status.clone()),
            printer_state: Set(update.printer_state.clone()),
            progress_percent: Set(update.progress_percent.map(i32::from)),
            remaining_time_minutes: Set(update.remaining_time_minutes.map(|value| value as i32)),
            current_layer: Set(update.current_layer.map(|value| value as i32)),
            total_layers: Set(update.total_layers.map(|value| value as i32)),
            active_file: Set(update.active_file.clone()),
            last_progress_percent: Set(update.last_progress_percent.map(i32::from)),
            last_layer: Set(update.last_layer.map(|value| value as i32)),
            print_error: Set(update.print_error.clone()),
            print_started_at: Set(update.print_started_at.clone()),
            print_finished_at: Set(update.print_finished_at.clone()),
            print_updated_at: Set(update.print_updated_at.clone()),
            updated_at: Set(pandar_core::created_at_now()),
            ..Default::default()
        })
        .filter(jobs::Column::Id.eq(job_id.to_string()))
        .filter(jobs::Column::PrintStatus.is_not_in(["completed", "failed", "cancelled"]))
        .exec(connection)
        .await
        .context("failed to update job print state")?;
    Ok(result.rows_affected > 0)
}

fn incoming_status(state: Option<&str>, current: Option<&PrintStatus>) -> Option<PrintStatus> {
    match state.map(str::trim) {
        Some("RUNNING") => Some(PrintStatus::Running),
        Some("FINISH") => Some(PrintStatus::Completed),
        Some("FAILED") => Some(PrintStatus::Failed),
        Some("IDLE") if current == Some(&PrintStatus::Running) => Some(PrintStatus::Cancelled),
        _ => None,
    }
}

fn terminal_error(input: &ApplyPrintReport) -> Option<String> {
    input
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.kind == "print_error" && !diagnostic.message.trim().is_empty()
        })
        .map(|diagnostic| diagnostic.message.trim().to_string())
        .or_else(|| {
            input
                .diagnostics
                .iter()
                .find(|diagnostic| !diagnostic.message.trim().is_empty())
                .map(|diagnostic| diagnostic.message.trim().to_string())
        })
}
