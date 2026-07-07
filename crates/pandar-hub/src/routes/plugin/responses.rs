use crate::{
    artifacts::metadata::ArtifactMetadata,
    repositories::{JobWithArtifact, RepositoryError},
};

use super::{PluginJobResponse, PluginPrintResponse};

impl TryFrom<JobWithArtifact> for PluginJobResponse {
    type Error = RepositoryError;

    fn try_from(value: JobWithArtifact) -> Result<Self, Self::Error> {
        Ok(Self {
            task_id: value.job.id.to_string(),
            dev_id: value.job.printer_id,
            name: value.artifact.filename,
            status: value.job.status.to_string(),
            progress_percent: value.job.print.progress_percent,
            artifact_metadata: artifact_metadata(value.artifact.metadata_json)?,
            created_at: value.job.created_at,
            updated_at: value.job.updated_at,
            pandar_job_id: value.job.id.to_string(),
        })
    }
}

impl TryFrom<JobWithArtifact> for PluginPrintResponse {
    type Error = RepositoryError;

    fn try_from(value: JobWithArtifact) -> Result<Self, Self::Error> {
        Ok(Self {
            task_id: value.job.id.to_string(),
            command_id: value.job.command_id.to_string(),
            status: value.job.status.to_string(),
            message: None,
            artifact_metadata: artifact_metadata(value.artifact.metadata_json)?,
            pandar_job_id: value.job.id.to_string(),
        })
    }
}

fn artifact_metadata(
    metadata_json: Option<String>,
) -> Result<Option<ArtifactMetadata>, RepositoryError> {
    metadata_json
        .map(|value| serde_json::from_str::<ArtifactMetadata>(&value))
        .transpose()
        .map_err(|err| {
            RepositoryError::Database(
                anyhow::Error::new(err).context("invalid persisted artifact metadata"),
            )
        })
}

pub(crate) fn redact_artifact_error(message: &str) -> String {
    message
        .lines()
        .map(|line| {
            if line.contains("artifact directory ")
                || line.contains("artifact file ")
                || line.contains("artifact storage path ")
            {
                line.split_once("artifact")
                    .map(|(prefix, suffix)| {
                        format!("{prefix}artifact{}", redact_artifact_path(suffix))
                    })
                    .unwrap_or_else(|| line.to_owned())
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_artifact_path(suffix: &str) -> String {
    for marker in [" directory ", " file ", " storage path "] {
        if let Some((prefix, _)) = suffix.split_once(marker) {
            return format!("{prefix}{marker}[redacted]");
        }
    }
    suffix.to_owned()
}
