use crate::{
    entities::{job_artifacts, jobs},
    repositories::{CreatePrintJob, RepositoryResult},
};
use anyhow::Context;
use pandar_core::{CommandId, JobId, JobStatus, PrintStatus, StudioSubmissionId};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait};

pub(super) async fn insert_artifact<C>(
    connection: &C,
    input: &CreatePrintJob,
    now: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    job_artifacts::ActiveModel {
        id: Set(input.artifact.id.clone()),
        tenant_id: Set(input.tenant_id.to_string()),
        filename: Set(input.artifact.filename.clone()),
        content_type: Set(input.artifact.content_type.clone()),
        size_bytes: Set(input.artifact.size_bytes as i64),
        storage_path: Set(input.artifact.storage_path.clone()),
        metadata_json: Set(input.artifact.metadata_json.clone()),
        created_at: Set(now.to_owned()),
    }
    .insert(connection)
    .await
    .context("failed to insert job artifact")?;
    Ok(())
}

pub(super) async fn insert_job<C>(
    connection: &C,
    input: &CreatePrintJob,
    job_id: JobId,
    command_id: CommandId,
    studio_submission_id: StudioSubmissionId,
    studio_metadata_json: Option<&str>,
    now: &str,
) -> RepositoryResult<()>
where
    C: ConnectionTrait,
{
    jobs::ActiveModel {
        id: Set(job_id.to_string()),
        tenant_id: Set(input.tenant_id.to_string()),
        printer_id: Set(input.printer_id.clone()),
        agent_id: Set(input.agent_id.to_string()),
        artifact_id: Set(input.artifact.id.clone()),
        command_id: Set(command_id.to_string()),
        studio_submission_id: Set(studio_submission_id.get()),
        plate_index: Set(
            i32::try_from(input.options.plate_id).context("plate index exceeds int32")?
        ),
        studio_metadata_json: Set(studio_metadata_json.map(str::to_owned)),
        status: Set(JobStatus::Queued.as_str().to_owned()),
        error: Set(None),
        created_at: Set(now.to_owned()),
        updated_at: Set(now.to_owned()),
        print_status: Set(PrintStatus::Pending.as_str().to_owned()),
        ams_mapping_json: Set(input.options.ams_mapping_json.clone()),
        ams_mapping2_json: Set(input.options.ams_mapping2_json.clone()),
        ams_mapping_info_json: Set(input.options.ams_mapping_info_json.clone()),
        ..Default::default()
    }
    .insert(connection)
    .await
    .context("failed to insert print job")?;
    Ok(())
}
