use anyhow::Context;
use pandar_core::Printer;
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet, ActiveValue::Set, ConnectionTrait};
use serde::{Deserialize, Serialize};

use crate::{
    entities::printers,
    repositories::{RepositoryError, RepositoryResult},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterHms {
    pub attr: u32,
    pub code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterLiveStatus {
    pub gcode_state: Option<String>,
    pub task_id: Option<String>,
    pub subtask_id: Option<String>,
    pub progress_percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub print_error: Option<u32>,
    pub printer_job_id: Option<String>,
    pub hms: Vec<PrinterHms>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterWithLiveStatus {
    pub printer: Printer,
    pub live_status: PrinterLiveStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrinterLiveStatusPatch {
    pub task_id: Option<String>,
    pub subtask_id: Option<String>,
    pub progress_percent: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub print_error: Option<u32>,
    pub printer_job_id: Option<String>,
    pub gcode_state: Option<String>,
    pub hms: Option<Vec<PrinterHms>>,
    pub observed_at: String,
}

pub(super) fn from_model(model: printers::Model) -> RepositoryResult<PrinterWithLiveStatus> {
    let live_status = (|| -> anyhow::Result<PrinterLiveStatus> {
        Ok(PrinterLiveStatus {
            gcode_state: model.print_gcode_state.clone(),
            task_id: model.print_task_id.clone(),
            subtask_id: model.print_subtask_id.clone(),
            progress_percent: model
                .print_progress_percent
                .map(u8::try_from)
                .transpose()
                .context("failed to read printer progress percent")?,
            remaining_time_minutes: model
                .print_remaining_time_minutes
                .map(u32::try_from)
                .transpose()
                .context("failed to read printer remaining time")?,
            current_layer: model
                .print_current_layer
                .map(u32::try_from)
                .transpose()
                .context("failed to read printer current layer")?,
            total_layers: model
                .print_total_layers
                .map(u32::try_from)
                .transpose()
                .context("failed to read printer total layers")?,
            gcode_file: model.print_gcode_file.clone(),
            subtask_name: model.print_subtask_name.clone(),
            print_error: model
                .print_error
                .map(u32::try_from)
                .transpose()
                .context("failed to read printer print error")?,
            printer_job_id: model.print_job_id.clone(),
            hms: serde_json::from_str(&model.hms_json).context("failed to read printer HMS")?,
        })
    })()
    .context("failed to rehydrate printer live status")
    .map_err(RepositoryError::from)?;

    Ok(PrinterWithLiveStatus {
        printer: super::printer_from_model(model)?,
        live_status,
    })
}

pub(crate) async fn update_in_connection<C>(
    connection: &C,
    printer_id: &str,
    patch: PrinterLiveStatusPatch,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    let mut active = printers::ActiveModel {
        id: Set(printer_id.to_string()),
        last_seen_at: Set(Some(patch.observed_at)),
        ..Default::default()
    };
    active.print_task_id = patch
        .task_id
        .map(|value| Set(Some(value)))
        .unwrap_or(NotSet);
    active.print_subtask_id = patch
        .subtask_id
        .map(|value| Set(Some(value)))
        .unwrap_or(NotSet);
    active.print_progress_percent = patch
        .progress_percent
        .map(|value| Set(Some(i64::from(value))))
        .unwrap_or(NotSet);
    active.print_remaining_time_minutes = patch
        .remaining_time_minutes
        .map(|value| Set(Some(i64::from(value))))
        .unwrap_or(NotSet);
    active.print_current_layer = patch
        .current_layer
        .map(|value| Set(Some(i64::from(value))))
        .unwrap_or(NotSet);
    active.print_total_layers = patch
        .total_layers
        .map(|value| Set(Some(i64::from(value))))
        .unwrap_or(NotSet);
    active.print_gcode_file = patch
        .gcode_file
        .map(|value| Set(Some(value)))
        .unwrap_or(NotSet);
    active.print_subtask_name = patch
        .subtask_name
        .map(|value| Set(Some(value)))
        .unwrap_or(NotSet);
    active.print_error = match patch.print_error {
        Some(value) => Set(Some(
            i32::try_from(value).context("failed to persist printer print error")?,
        )),
        None => NotSet,
    };
    active.print_job_id = patch
        .printer_job_id
        .map(|value| Set(Some(value)))
        .unwrap_or(NotSet);
    active.print_gcode_state = patch
        .gcode_state
        .map(|value| Set(Some(value)))
        .unwrap_or(NotSet);
    active.hms_json = match patch.hms {
        Some(hms) => Set(serde_json::to_string(&hms).context("failed to serialize printer HMS")?),
        None => NotSet,
    };
    active
        .update(connection)
        .await
        .context("failed to update printer live status")?;

    Ok(())
}
