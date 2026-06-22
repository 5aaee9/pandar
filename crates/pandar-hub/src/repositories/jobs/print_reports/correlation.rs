use anyhow::Context;
use pandar_core::{JobId, TenantId};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::{jobs, printers},
    repositories::{
        JobWithArtifact, RepositoryResult,
        jobs::{
            artifact_for_job, hydrate_jobs_with_artifacts, rows::job_with_artifact_from_models,
        },
    },
};

use super::ApplyPrintReport;

#[derive(Debug, Clone)]
pub(super) struct PrinterMatch {
    pub(super) id: String,
}

pub(super) async fn printer_for_serial<C>(
    connection: &C,
    input: &ApplyPrintReport,
) -> RepositoryResult<Option<PrinterMatch>>
where
    C: ConnectionTrait,
{
    printers::Entity::find()
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(printers::Column::SerialNumber.eq(&input.serial))
        .one(connection)
        .await
        .context("failed to resolve print report printer")
        .map(|printer| printer.map(|printer| PrinterMatch { id: printer.id }))
        .map_err(Into::into)
}

pub(super) async fn correlate_job<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Option<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    if let Some(job_id) = input.job_id
        && let Some(job) = job_by_id_for_printer(connection, input, printer, job_id).await?
    {
        return Ok(Some(job));
    }
    if let Some(job) =
        job_by_artifact(connection, input, printer, input.artifact_id.as_deref()).await?
    {
        return Ok(Some(job));
    }
    if let Some(job) =
        job_by_artifact(connection, input, printer, input.subtask_id.as_deref()).await?
    {
        return Ok(Some(job));
    }
    job_by_active_file(connection, input, printer).await
}

pub(super) async fn job_by_id<C>(
    connection: &C,
    tenant_id: TenantId,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    let Some(job) = jobs::Entity::find_by_id(job_id.to_string())
        .filter(jobs::Column::TenantId.eq(tenant_id.to_string()))
        .one(connection)
        .await
        .context("failed to get print report job")?
    else {
        return Ok(None);
    };

    let artifact = artifact_for_job(connection, &job).await?;
    Ok(Some(job_with_artifact_from_models(job, artifact)?))
}

async fn job_by_id_for_printer<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    job_id: JobId,
) -> RepositoryResult<Option<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    let Some(job) = jobs::Entity::find_by_id(job_id.to_string())
        .filter(jobs::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(jobs::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(jobs::Column::PrinterId.eq(&printer.id))
        .one(connection)
        .await
        .context("failed to correlate print report by job id")?
    else {
        return Ok(None);
    };

    let artifact = artifact_for_job(connection, &job).await?;
    Ok(Some(job_with_artifact_from_models(job, artifact)?))
}

async fn job_by_artifact<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
    artifact_id: Option<&str>,
) -> RepositoryResult<Option<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    let Some(artifact_id) = artifact_id else {
        return Ok(None);
    };
    let Some(job) = jobs::Entity::find()
        .filter(jobs::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(jobs::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(jobs::Column::PrinterId.eq(&printer.id))
        .filter(jobs::Column::ArtifactId.eq(artifact_id))
        .one(connection)
        .await
        .context("failed to correlate print report by artifact id")?
    else {
        return Ok(None);
    };

    let artifact = artifact_for_job(connection, &job).await?;
    Ok(Some(job_with_artifact_from_models(job, artifact)?))
}

async fn job_by_active_file<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Option<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    let candidates = active_file_candidates(connection, input, printer).await?;
    Ok(single_file_match(candidates, input))
}

async fn active_file_candidates<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Vec<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    let cutoff = cutoff_observed_at(&input.observed_at)?;
    let job_models = jobs::Entity::find()
        .filter(jobs::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(jobs::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(jobs::Column::PrinterId.eq(&printer.id))
        .filter(jobs::Column::PrintStatus.is_in(["pending", "running"]))
        .filter(jobs::Column::CreatedAt.gte(cutoff))
        .all(connection)
        .await
        .context("failed to list active-file print report candidates")?;

    hydrate_jobs_with_artifacts(connection, job_models).await
}

fn single_file_match(
    candidates: Vec<JobWithArtifact>,
    input: &ApplyPrintReport,
) -> Option<JobWithArtifact> {
    let report_basename = input.gcode_file.as_deref().and_then(basename);
    let subtask_name = input
        .subtask_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut matches = candidates.into_iter().filter(|candidate| {
        let filename = candidate.artifact.filename.trim();
        report_basename.is_some_and(|name| name == filename)
            || subtask_name.is_some_and(|name| name == filename_stem(filename))
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn cutoff_observed_at(observed_at: &str) -> RepositoryResult<String> {
    let observed = OffsetDateTime::parse(observed_at, &Rfc3339)
        .context("failed to parse print report observed_at")?;
    (observed - Duration::hours(24))
        .format(&Rfc3339)
        .context("failed to format print report fallback cutoff")
        .map_err(Into::into)
}

fn basename(value: &str) -> Option<&str> {
    value
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .filter(|value| !value.is_empty())
}

fn filename_stem(filename: &str) -> &str {
    filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename)
}
