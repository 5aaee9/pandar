use anyhow::Context;
use pandar_core::{
    AgentId, CommandId, Job, JobArtifact, JobArtifactParts, JobId, JobParts, TenantId,
};

use crate::{
    entities::{job_artifacts, jobs},
    repositories::{JobWithArtifact, RepositoryError, RepositoryResult},
};

pub fn job_with_artifact_from_models(
    job: jobs::Model,
    artifact: job_artifacts::Model,
) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        job: job_from_model(job)?,
        artifact: artifact_from_model(artifact)?,
    })
}

pub fn job_from_model(model: jobs::Model) -> RepositoryResult<Job> {
    let status_for_error = model.status.clone();
    let print_status_for_error = model.print_status.clone();
    Job::from_parts(JobParts {
        id: JobId::parse(&model.id).map_err(anyhow::Error::from)?,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        printer_id: model.printer_id,
        agent_id: AgentId::parse(&model.agent_id).map_err(anyhow::Error::from)?,
        artifact_id: model.artifact_id,
        command_id: CommandId::parse(&model.command_id).map_err(anyhow::Error::from)?,
        status: model.status,
        error: model.error,
        print_status: model.print_status,
        printer_state: model.printer_state,
        progress_percent: model.progress_percent.map(i32_to_u8).transpose()?,
        remaining_time_minutes: model.remaining_time_minutes.map(i32_to_u32).transpose()?,
        current_layer: model.current_layer.map(i32_to_u32).transpose()?,
        total_layers: model.total_layers.map(i32_to_u32).transpose()?,
        active_file: model.active_file,
        last_progress_percent: model.last_progress_percent.map(i32_to_u8).transpose()?,
        last_layer: model.last_layer.map(i32_to_u32).transpose()?,
        print_error: model.print_error,
        print_started_at: model.print_started_at,
        print_finished_at: model.print_finished_at,
        print_updated_at: model.print_updated_at,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
    .map_err(|err| match err {
        pandar_core::CoreError::InvalidJobStatus(_) => {
            RepositoryError::InvalidPersistedJobStatus(status_for_error)
        }
        pandar_core::CoreError::InvalidPrintStatus(_) => {
            RepositoryError::InvalidPersistedPrintStatus(print_status_for_error)
        }
        err => {
            RepositoryError::Database(anyhow::Error::from(err).context("failed to rehydrate job"))
        }
    })
}

fn artifact_from_model(model: job_artifacts::Model) -> RepositoryResult<JobArtifact> {
    JobArtifact::from_parts(JobArtifactParts {
        id: model.id,
        tenant_id: TenantId::parse(&model.tenant_id).map_err(anyhow::Error::from)?,
        filename: model.filename,
        content_type: model.content_type,
        size_bytes: model.size_bytes as u64,
        storage_path: model.storage_path,
        created_at: model.created_at,
    })
    .map_err(anyhow::Error::from)
    .context("failed to rehydrate job artifact")
    .map_err(RepositoryError::from)
}

fn i32_to_u8(value: i32) -> RepositoryResult<u8> {
    u8::try_from(value).map_err(|err| {
        RepositoryError::Database(
            anyhow::Error::from(err).context("invalid persisted u8 value for job print state"),
        )
    })
}

fn i32_to_u32(value: i32) -> RepositoryResult<u32> {
    u32::try_from(value).map_err(|err| {
        RepositoryError::Database(
            anyhow::Error::from(err).context("invalid persisted u32 value for job print state"),
        )
    })
}
