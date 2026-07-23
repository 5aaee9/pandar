use pandar_core::StudioPrintMetadata;
use serde::Serialize;

use crate::{repositories::JobWithArtifact, routes::ApiError};

#[derive(Debug, Serialize)]
pub(crate) struct StudioModelTaskResponse {
    job_id: i32,
    design_id: i32,
    profile_id: i32,
    instance_id: i32,
    task_id: String,
    model_id: String,
    model_name: String,
    profile_name: String,
}

pub(super) fn from_job(value: JobWithArtifact) -> Result<StudioModelTaskResponse, ApiError> {
    let metadata = match value.job.studio_metadata.as_ref() {
        Some(StudioPrintMetadata::V1(metadata)) => metadata,
        None => return Err(unavailable()),
    };
    if metadata.project_name.trim().is_empty()
        || metadata.preset_name.trim().is_empty()
        || metadata.stl_design_id != 0
        || metadata.origin_profile_id != 0
        || !metadata.origin_model_id.is_empty()
    {
        return Err(unavailable());
    }
    let id = value.job.studio_submission_id.get();
    Ok(StudioModelTaskResponse {
        job_id: id,
        design_id: 0,
        profile_id: 0,
        instance_id: 0,
        task_id: id.to_string(),
        model_id: String::new(),
        model_name: metadata.project_name.clone(),
        profile_name: metadata.preset_name.clone(),
    })
}

pub(super) fn unavailable() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "studio_model_task_metadata_unavailable",
    )
}
