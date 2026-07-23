use pandar_core::{Job, JobStatus, PrintStatus, Printer, StudioPrintMetadata};
use serde::Serialize;

use crate::repositories::JobWithArtifact;

#[derive(Debug, Serialize)]
pub(crate) struct StudioCreatePrintResponse {
    task_id: i32,
    studio_submission_id: i32,
    status: String,
}

impl From<&JobWithArtifact> for StudioCreatePrintResponse {
    fn from(value: &JobWithArtifact) -> Self {
        let id = value.job.studio_submission_id.get();
        Self {
            task_id: id,
            studio_submission_id: id,
            status: value.job.status.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct StudioTaskPageResponse {
    pub(super) total: u64,
    pub(super) hits: Vec<StudioTaskHit>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StudioTaskHit {
    id: i32,
    status: i32,
    design_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    design_title: Option<String>,
    device_name: String,
    device_id: String,
    cover: String,
    start_time: String,
    end_time: String,
    profile_id: i64,
}

pub(super) fn task_hit_from_job(value: JobWithArtifact, printer: &Printer) -> StudioTaskHit {
    let id = value.job.studio_submission_id.get();
    let (task_name, origin_profile_id, stl_design_id) = match &value.job.studio_metadata {
        Some(StudioPrintMetadata::V1(metadata)) => {
            let title = if metadata.task_name.trim().is_empty() {
                artifact_basename(&value.artifact.filename)
            } else {
                metadata.task_name.clone()
            };
            (title, metadata.origin_profile_id, metadata.stl_design_id)
        }
        None => (artifact_basename(&value.artifact.filename), 0, 0),
    };
    let design_id = stl_design_id.max(0);
    StudioTaskHit {
        id,
        status: studio_status(&value.job),
        design_id,
        title: (design_id == 0).then_some(task_name.clone()),
        design_title: (design_id > 0).then_some(task_name),
        device_name: printer.name.clone(),
        device_id: printer.serial_number.clone(),
        cover: String::new(),
        start_time: value.job.created_at,
        end_time: value.job.print.finished_at.unwrap_or_default(),
        profile_id: if origin_profile_id > 0 {
            origin_profile_id
        } else {
            i64::from(id)
        },
    }
}

fn studio_status(job: &Job) -> i32 {
    if job.print.status == PrintStatus::Completed {
        2
    } else if job.status == JobStatus::Failed
        || matches!(
            job.print.status,
            PrintStatus::Failed | PrintStatus::Cancelled
        )
    {
        3
    } else {
        1
    }
}

fn artifact_basename(value: &str) -> String {
    value.rsplit(['/', '\\']).next().unwrap_or(value).to_owned()
}

#[derive(Debug, Serialize)]
pub(crate) struct StudioTaskDetailResponse {
    studio_submission_id: i32,
    job_status: String,
    print_status: String,
}

impl From<&Job> for StudioTaskDetailResponse {
    fn from(value: &Job) -> Self {
        Self {
            studio_submission_id: value.studio_submission_id.get(),
            job_status: value.status.to_string(),
            print_status: value.print.status.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct StudioPlateResponse {
    studio_submission_id: i32,
    plate_index: u32,
}

impl From<&Job> for StudioPlateResponse {
    fn from(value: &Job) -> Self {
        Self {
            studio_submission_id: value.studio_submission_id.get(),
            plate_index: value.plate_index,
        }
    }
}
