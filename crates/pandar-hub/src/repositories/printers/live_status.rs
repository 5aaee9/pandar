use anyhow::Context;
use pandar_core::Printer;
use serde::{Deserialize, Serialize};

use crate::{
    entities::printers,
    printer_secrets::PrinterAccessCodeCipher,
    repositories::{RepositoryError, RepositoryResult},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrinterHms {
    pub attr: u32,
    pub code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterLiveStatus {
    pub task_generation: u64,
    pub error_generation: u64,
    pub job_attr: Option<u32>,
    pub error_task_generation: Option<u64>,
    pub error_session_id: Option<String>,
    pub error_received_at: Option<String>,
    pub gcode_state: Option<String>,
    pub task_id: Option<String>,
    pub subtask_id: Option<String>,
    pub progress_percent: Option<u8>,
    pub speed_level: Option<u8>,
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
    pub state_revision: u64,
    pub printer: Printer,
    pub live_status: PrinterLiveStatus,
    pub firmware: pandar_core::PrinterFirmwareState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PrinterLiveStatusPatch {
    pub task_id: Option<String>,
    pub subtask_id: Option<String>,
    pub progress_percent: Option<u8>,
    pub speed_level: Option<u8>,
    pub remaining_time_minutes: Option<u32>,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub print_error: Option<u32>,
    pub printer_job_id: Option<String>,
    pub job_attr: Option<u32>,
    pub gcode_state: Option<String>,
    pub hms: Option<Vec<PrinterHms>>,
    pub observed_at: String,
}

pub(crate) mod merge;
mod persistence;

pub(crate) use merge::merge_live_report;
pub(crate) use persistence::persist_merged_live_status;

pub(crate) fn from_model(
    model: printers::Model,
    access_code_cipher: &PrinterAccessCodeCipher,
) -> RepositoryResult<PrinterWithLiveStatus> {
    let firmware = super::firmware::from_model(&model)?;
    let (state_revision, live_status) = (|| -> anyhow::Result<(u64, PrinterLiveStatus)> {
        let state_revision =
            u64::try_from(model.state_revision).context("failed to read printer state revision")?;
        let state_revision = std::num::NonZeroU64::new(state_revision)
            .context("failed to read printer state revision")?
            .get();
        Ok((
            state_revision,
            PrinterLiveStatus {
                task_generation: u64::try_from(model.print_task_generation)
                    .context("failed to read printer task generation")?,
                error_generation: u64::try_from(model.print_error_generation)
                    .context("failed to read printer error generation")?,
                job_attr: model
                    .print_job_attr
                    .map(u32::try_from)
                    .transpose()
                    .context("failed to read printer job attr")?,
                error_task_generation: model
                    .print_error_task_generation
                    .map(u64::try_from)
                    .transpose()
                    .context("failed to read printer error task generation")?,
                error_session_id: model.print_error_session_id.clone(),
                error_received_at: model.print_error_received_at.clone(),
                gcode_state: model.print_gcode_state.clone(),
                task_id: model.print_task_id.clone(),
                subtask_id: model.print_subtask_id.clone(),
                progress_percent: model
                    .print_progress_percent
                    .map(u8::try_from)
                    .transpose()
                    .context("failed to read printer progress percent")?,
                speed_level: model
                    .print_speed_level
                    .map(u8::try_from)
                    .transpose()
                    .context("failed to read printer speed level")?,
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
            },
        ))
    })()
    .context("failed to rehydrate printer live status")
    .map_err(RepositoryError::from)?;

    Ok(PrinterWithLiveStatus {
        state_revision,
        printer: super::printer_from_model(model, access_code_cipher)?,
        live_status,
        firmware,
    })
}
