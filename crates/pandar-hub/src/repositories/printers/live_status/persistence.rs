use anyhow::Context;
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    sea_query::{Expr, ExprTrait},
};

use crate::{
    entities::printers,
    repositories::{RepositoryError, RepositoryResult},
};

use super::PrinterLiveStatus;

pub(crate) async fn persist_merged_live_status<C>(
    connection: &C,
    printer_id: &str,
    state: &PrinterLiveStatus,
    observed_at: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    printers::Entity::update_many()
        .filter(printers::Column::Id.eq(printer_id))
        .set(merged_active_model(state, observed_at)?)
        .col_expr(
            printers::Column::StateRevision,
            Expr::col(printers::Column::StateRevision).add(1),
        )
        .exec(connection)
        .await
        .context("failed to persist merged printer live status")?;
    Ok(())
}

fn merged_active_model(
    state: &PrinterLiveStatus,
    observed_at: &str,
) -> RepositoryResult<printers::ActiveModel> {
    (|| -> anyhow::Result<printers::ActiveModel> {
        Ok(printers::ActiveModel {
            last_seen_at: Set(Some(observed_at.to_owned())),
            print_task_generation: Set(i64::try_from(state.task_generation)
                .context("failed to persist printer task generation")?),
            print_error_generation: Set(i64::try_from(state.error_generation)
                .context("failed to persist printer error generation")?),
            print_job_attr: Set(state.job_attr.map(i64::from)),
            print_error_task_generation: Set(state
                .error_task_generation
                .map(i64::try_from)
                .transpose()
                .context("failed to persist printer error task generation")?),
            print_error_session_id: Set(state.error_session_id.clone()),
            print_error_received_at: Set(state.error_received_at.clone()),
            print_gcode_state: Set(state.gcode_state.clone()),
            print_task_id: Set(state.task_id.clone()),
            print_subtask_id: Set(state.subtask_id.clone()),
            print_progress_percent: Set(state.progress_percent.map(i64::from)),
            print_remaining_time_minutes: Set(state.remaining_time_minutes.map(i64::from)),
            print_current_layer: Set(state.current_layer.map(i64::from)),
            print_total_layers: Set(state.total_layers.map(i64::from)),
            print_gcode_file: Set(state.gcode_file.clone()),
            print_subtask_name: Set(state.subtask_name.clone()),
            print_error: Set(state
                .print_error
                .map(i32::try_from)
                .transpose()
                .context("failed to persist printer print error")?),
            print_job_id: Set(state.printer_job_id.clone()),
            hms_json: Set(
                serde_json::to_string(&state.hms).context("failed to serialize printer HMS")?
            ),
            ..Default::default()
        })
    })()
    .map_err(RepositoryError::from)
}
