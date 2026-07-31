use anyhow::Context;
use pandar_core::PrintStatus;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, EntityTrait, FromQueryResult, JoinType, QueryFilter,
    QueryOrder, QuerySelect, RelationDef,
};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::{commands, jobs},
    repositories::{JobRepository, JobWithArtifact, RepositoryResult},
};

use super::hydration::hydrate_jobs_with_artifacts;
use crate::db::ConnectionDialectExt;

pub(super) const STALLED_JOB_AGE: Duration = Duration::minutes(15);
const STALL_SWEEP_BATCH_SIZE: u64 = 500;

#[derive(FromQueryResult)]
struct PendingCommandCandidate {
    command_id: String,
    updated_at: String,
}

impl JobRepository {
    pub async fn mark_stalled_pending_jobs(
        &self,
        now: &str,
    ) -> RepositoryResult<Vec<JobWithArtifact>> {
        let now = OffsetDateTime::parse(now, &Rfc3339)
            .context("failed to parse pending print stall sweep time")?;
        let query_cutoff = (now - STALLED_JOB_AGE + Duration::seconds(1))
            .format(&Rfc3339)
            .context("failed to format pending print stall query cutoff")?;
        let transaction = self
            .database
            .begin_write_transaction()
            .await
            .context("failed to begin pending print stall transaction")?;
        let command_relation: RelationDef = jobs::Entity::belongs_to(commands::Entity)
            .from(jobs::Column::CommandId)
            .to(commands::Column::Id)
            .into();
        let possibly_aged_commands = jobs::Entity::find()
            .join(JoinType::InnerJoin, command_relation)
            .select_only()
            .column_as(commands::Column::Id, "command_id")
            .column(commands::Column::UpdatedAt)
            .filter(jobs::Column::Status.eq("succeeded"))
            .filter(jobs::Column::PrintStatus.eq(PrintStatus::Pending.as_str()))
            .filter(jobs::Column::PrintStartedAt.is_null())
            .filter(
                Condition::any()
                    .add(jobs::Column::ProgressPercent.is_null())
                    .add(jobs::Column::ProgressPercent.eq(0)),
            )
            .filter(
                Condition::any()
                    .add(jobs::Column::CurrentLayer.is_null())
                    .add(jobs::Column::CurrentLayer.eq(0)),
            )
            .filter(commands::Column::Kind.eq("print_project_file"))
            .filter(commands::Column::Status.eq("succeeded"))
            .filter(commands::Column::UpdatedAt.lt(query_cutoff))
            .order_by_asc(commands::Column::UpdatedAt)
            .limit(STALL_SWEEP_BATCH_SIZE)
            .into_model::<PendingCommandCandidate>()
            .all(&transaction)
            .await
            .context("failed to list aged successful print commands")?;
        let mut aged_command_ids = Vec::with_capacity(possibly_aged_commands.len());
        for command in possibly_aged_commands {
            if dispatch_succeeded_at_is_stalled(&command.command_id, &command.updated_at, now)? {
                aged_command_ids.push(command.command_id);
            }
        }
        if aged_command_ids.is_empty() {
            transaction
                .commit()
                .await
                .context("failed to commit empty pending print stall transaction")?;
            return Ok(Vec::new());
        }
        let candidates = jobs::Entity::find()
            .filter(jobs::Column::Status.eq("succeeded"))
            .filter(jobs::Column::PrintStatus.eq(PrintStatus::Pending.as_str()))
            .filter(jobs::Column::PrintStartedAt.is_null())
            .filter(
                Condition::any()
                    .add(jobs::Column::ProgressPercent.is_null())
                    .add(jobs::Column::ProgressPercent.eq(0)),
            )
            .filter(
                Condition::any()
                    .add(jobs::Column::CurrentLayer.is_null())
                    .add(jobs::Column::CurrentLayer.eq(0)),
            )
            .filter(jobs::Column::CommandId.is_in(aged_command_ids));
        let mut stalled = transaction
            .lock_for_update(candidates)
            .all(&transaction)
            .await
            .context("failed to lock pending print stall candidates")?;
        if stalled.is_empty() {
            transaction
                .commit()
                .await
                .context("failed to commit empty pending print stall transaction")?;
            return Ok(Vec::new());
        }
        let candidate_ids = stalled.iter().map(|job| job.id.clone()).collect::<Vec<_>>();
        let updated = jobs::Entity::update_many()
            .set(jobs::ActiveModel {
                print_status: Set(PrintStatus::Stalled.as_str().to_owned()),
                ..Default::default()
            })
            .filter(jobs::Column::Id.is_in(candidate_ids))
            .filter(jobs::Column::PrintStatus.eq(PrintStatus::Pending.as_str()))
            .exec(&transaction)
            .await
            .context("failed to mark pending print jobs stalled")?;
        if updated.rows_affected != stalled.len() as u64 {
            return Err(anyhow::anyhow!(
                "pending print stall update changed {} of {} locked jobs",
                updated.rows_affected,
                stalled.len()
            )
            .into());
        }
        for job in &mut stalled {
            job.print_status = PrintStatus::Stalled.as_str().to_owned();
        }
        let stalled = hydrate_jobs_with_artifacts(&transaction, stalled).await?;
        transaction
            .commit()
            .await
            .context("failed to commit pending print stall transaction")?;
        Ok(stalled)
    }
}

pub(super) fn pending_job_is_stalled(
    command: &commands::Model,
    now: OffsetDateTime,
) -> RepositoryResult<bool> {
    dispatch_succeeded_at_is_stalled(&command.id, &command.updated_at, now)
}

fn dispatch_succeeded_at_is_stalled(
    command_id: &str,
    updated_at: &str,
    now: OffsetDateTime,
) -> RepositoryResult<bool> {
    let dispatch_succeeded_at = OffsetDateTime::parse(updated_at, &Rfc3339).with_context(|| {
        format!("failed to parse dispatch completion time for command {command_id}")
    })?;
    Ok(dispatch_succeeded_at < now - STALLED_JOB_AGE)
}
