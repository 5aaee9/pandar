use std::collections::HashSet;

use anyhow::Context;
use pandar_core::{CommandStatus, JobId, JobStatus, PrintStatus, TenantId};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};
use serde::Deserialize;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    entities::{commands, jobs, printers},
    printer_secrets::PrinterAccessCodeCipher,
    repositories::{
        JobWithArtifact, PrinterLiveStatus, RepositoryResult,
        jobs::hydration::hydrate_jobs_with_artifacts, printers::live_status_from_model,
    },
};

use super::ApplyPrintReport;

#[derive(Debug, Clone)]
pub(super) struct PrinterMatch {
    pub(super) id: String,
    pub(super) live_status: PrinterLiveStatus,
}

#[derive(Deserialize)]
struct PersistedPrintProjectResult {
    #[serde(rename = "type")]
    kind: String,
    mqtt: PersistedPrintProjectMqtt,
}

#[derive(Deserialize)]
struct PersistedPrintProjectMqtt {
    payload: PersistedProjectFileEnvelope,
}

#[derive(Deserialize)]
struct PersistedProjectFileEnvelope {
    print: PersistedProjectFileIdentity,
}

#[derive(Deserialize)]
struct PersistedProjectFileIdentity {
    command: String,
    task_id: String,
}

pub(super) async fn printer_for_serial<C>(
    connection: &C,
    access_code_cipher: &PrinterAccessCodeCipher,
    input: &ApplyPrintReport,
) -> RepositoryResult<Option<PrinterMatch>>
where
    C: ConnectionTrait,
{
    let query = printers::Entity::find()
        .filter(printers::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(printers::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(printers::Column::SerialNumber.eq(&input.serial));
    match connection.get_database_backend() {
        sea_orm::DatabaseBackend::Postgres => query.lock_exclusive().one(connection).await,
        _ => query.one(connection).await,
    }
    .context("failed to resolve print report printer")?
    .map(|model| live_status_from_model(model, access_code_cipher))
    .transpose()
    .map(|printer| {
        printer.map(|printer| PrinterMatch {
            id: printer.printer.id,
            live_status: printer.live_status,
        })
    })
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
    let mut submission_matches = jobs_by_submission_id(connection, input, printer).await?;
    match submission_matches.len() {
        1 => return Ok(submission_matches.pop()),
        2.. => return Ok(None),
        0 => {}
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

    Ok(hydrate_jobs_with_artifacts(connection, vec![job])
        .await?
        .into_iter()
        .next())
}

async fn jobs_by_submission_id<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Vec<JobWithArtifact>>
where
    C: ConnectionTrait,
{
    let Some(task_id) = input.task_id.as_deref() else {
        return Ok(Vec::new());
    };
    let job_models = jobs::Entity::find()
        .filter(jobs::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(jobs::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(jobs::Column::PrinterId.eq(&printer.id))
        .filter(jobs::Column::PrintStatus.is_in(["pending", "stalled", "running"]))
        .all(connection)
        .await
        .context("failed to list print submission report candidates")?
        .into_iter()
        .filter(|job| job.status == JobStatus::Succeeded.as_str())
        .collect::<Vec<_>>();
    if job_models.is_empty() {
        return Ok(Vec::new());
    }
    let command_models = commands::Entity::find()
        .filter(
            commands::Column::Id.is_in(
                job_models
                    .iter()
                    .map(|job| job.command_id.clone())
                    .collect::<Vec<_>>(),
            ),
        )
        .filter(commands::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(commands::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(commands::Column::PrinterId.eq(&printer.id))
        .filter(commands::Column::Kind.eq("print_project_file"))
        .filter(commands::Column::Status.eq(CommandStatus::Succeeded.as_str()))
        .filter(commands::Column::ResultJson.is_not_null())
        .all(connection)
        .await
        .context("failed to load print command results for report correlation")?;
    let matching_command_ids = command_models
        .into_iter()
        .filter_map(|command| {
            let result_json = command.result_json.as_deref()?;
            let result = match serde_json::from_str::<PersistedPrintProjectResult>(result_json) {
                Ok(result) => result,
                Err(error) => {
                    tracing::warn!(
                        command_id = %command.id,
                        error = %format!("{error:#}"),
                        "ignored invalid print command result during report correlation"
                    );
                    return None;
                }
            };
            (result.kind == "print_project_file"
                && result.mqtt.payload.print.command == "project_file"
                && result.mqtt.payload.print.task_id == task_id)
                .then_some(command.id)
        })
        .collect::<HashSet<_>>();
    let matches = job_models
        .into_iter()
        .filter(|job| matching_command_ids.contains(&job.command_id))
        .collect::<Vec<_>>();
    hydrate_jobs_with_artifacts(connection, matches).await
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

    Ok(hydrate_jobs_with_artifacts(connection, vec![job])
        .await?
        .into_iter()
        .next())
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

    Ok(hydrate_jobs_with_artifacts(connection, vec![job])
        .await?
        .into_iter()
        .next())
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
    let job_models = active_job_models(connection, input, printer).await?;
    hydrate_jobs_with_artifacts(connection, job_models).await
}

async fn active_job_models<C>(
    connection: &C,
    input: &ApplyPrintReport,
    printer: &PrinterMatch,
) -> RepositoryResult<Vec<jobs::Model>>
where
    C: ConnectionTrait,
{
    let cutoff = cutoff_observed_at(&input.observed_at)?;
    jobs::Entity::find()
        .filter(jobs::Column::TenantId.eq(input.tenant_id.to_string()))
        .filter(jobs::Column::AgentId.eq(input.agent_id.to_string()))
        .filter(jobs::Column::PrinterId.eq(&printer.id))
        .filter(jobs::Column::PrintStatus.is_in(["pending", "stalled", "running"]))
        .filter(jobs::Column::CreatedAt.gte(cutoff))
        .all(connection)
        .await
        .context("failed to list active print report candidates")
        .map_err(Into::into)
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
    let mut active_matches = Vec::new();
    let mut stalled_matches = Vec::new();
    for candidate in candidates {
        let filename = candidate.artifact.filename.trim();
        if report_basename.is_some_and(|name| name == filename)
            || subtask_name.is_some_and(|name| name == filename_stem(filename))
        {
            if candidate.job.print.status == PrintStatus::Stalled {
                stalled_matches.push(candidate);
            } else {
                active_matches.push(candidate);
            }
        }
    }
    match active_matches.len() {
        1 => active_matches.pop(),
        0 if stalled_matches.len() == 1 => stalled_matches.pop(),
        _ => None,
    }
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
