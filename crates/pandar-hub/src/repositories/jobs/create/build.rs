use super::NewPrintJobFromArtifact;
use crate::repositories::{CreatePrintJob, JobWithArtifact, RepositoryResult};
use anyhow::Context;
use pandar_core::{
    CommandId, Job, JobArtifact, JobArtifactParts, JobId, JobParts, JobStatus, PrintStatus,
    StudioSubmissionId,
};

pub(super) fn build_created_job(
    input: CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    studio_submission_id: StudioSubmissionId,
    studio_metadata_json: Option<String>,
    now: String,
) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        artifact: JobArtifact::from_parts(JobArtifactParts {
            id: input.artifact_id.clone(),
            tenant_id: input.tenant_id,
            filename: input.artifact_filename,
            content_type: input.artifact_content_type,
            size_bytes: input.artifact_size_bytes,
            storage_path: input.artifact_storage_path,
            metadata_json: input.artifact_metadata_json,
            created_at: now.clone(),
        })
        .map_err(anyhow::Error::from)
        .context("failed to build print job artifact")?,
        job: Job::from_parts(JobParts {
            id: job_id,
            tenant_id: input.tenant_id,
            printer_id: input.printer_id,
            agent_id: input.agent_id,
            artifact_id: input.artifact_id,
            command_id,
            studio_submission_id: i64::from(studio_submission_id),
            plate_index: input.plate_id,
            studio_metadata_json,
            status: JobStatus::Queued.as_str().to_owned(),
            error: None,
            print_status: PrintStatus::Pending.as_str().to_owned(),
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            print_error: None,
            print_started_at: None,
            print_finished_at: None,
            print_updated_at: None,
            ams_mapping_json: input.ams_mapping_json,
            ams_mapping2_json: input.ams_mapping2_json,
            ams_mapping_info_json: input.ams_mapping_info_json,
            filament_usage: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        })
        .map_err(anyhow::Error::from)
        .context("failed to build print job")?,
    })
}

pub(super) fn build_job_from_existing_artifact(
    input: NewPrintJobFromArtifact,
    job_id: JobId,
    command_id: CommandId,
    studio_submission_id: StudioSubmissionId,
    studio_metadata_json: Option<String>,
    now: String,
) -> RepositoryResult<JobWithArtifact> {
    Ok(JobWithArtifact {
        artifact: JobArtifact::from_parts(JobArtifactParts {
            id: input.artifact_id.clone(),
            tenant_id: input.tenant_id,
            filename: input.artifact_filename,
            content_type: input.artifact_content_type,
            size_bytes: input.artifact_size_bytes,
            storage_path: input.artifact_storage_path,
            metadata_json: input.artifact_metadata_json,
            created_at: now.clone(),
        })
        .map_err(anyhow::Error::from)
        .context("failed to build recovered print job artifact")?,
        job: Job::from_parts(JobParts {
            id: job_id,
            tenant_id: input.tenant_id,
            printer_id: input.printer_id,
            agent_id: input.agent_id,
            artifact_id: input.artifact_id,
            command_id,
            studio_submission_id: i64::from(studio_submission_id),
            plate_index: input.plate_id,
            studio_metadata_json,
            status: JobStatus::Queued.as_str().to_owned(),
            error: None,
            print_status: PrintStatus::Pending.as_str().to_owned(),
            printer_state: None,
            progress_percent: None,
            remaining_time_minutes: None,
            current_layer: None,
            total_layers: None,
            active_file: None,
            last_progress_percent: None,
            last_layer: None,
            print_error: None,
            print_started_at: None,
            print_finished_at: None,
            print_updated_at: None,
            ams_mapping_json: input.ams_mapping_json,
            ams_mapping2_json: input.ams_mapping2_json,
            ams_mapping_info_json: input.ams_mapping_info_json,
            filament_usage: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        })
        .map_err(anyhow::Error::from)
        .context("failed to build recovered print job")?,
    })
}
