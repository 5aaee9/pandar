use pandar_core::{Job, JobArtifact, JobFilamentUsage, JobPrintState};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{
    artifacts::metadata::ArtifactMetadata,
    material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo, validate_mapping_len},
    repositories::{JobWithArtifact, RepositoryError},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProjection {
    id: String,
    tenant_id: String,
    printer_id: String,
    agent_id: String,
    artifact_id: String,
    command_id: String,
    status: String,
    error: Option<String>,
    created_at: String,
    updated_at: String,
    print: EventJobPrint,
    command: EventJobCommand,
    artifact: EventJobArtifact,
    material: EventJobMaterial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventJobPrint {
    status: String,
    printer_state: Option<String>,
    progress_percent: Option<u8>,
    remaining_time_minutes: Option<u32>,
    current_layer: Option<u32>,
    total_layers: Option<u32>,
    active_file: Option<String>,
    last_progress_percent: Option<u8>,
    last_layer: Option<u32>,
    error: Option<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventJobCommand {
    id: String,
    kind: String,
    status: String,
    uploaded_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventJobArtifact {
    id: String,
    tenant_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    metadata: Option<ArtifactMetadata>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventJobMaterial {
    ams_mapping: Option<AmsMapping>,
    ams_mapping2: Option<AmsMapping2>,
    ams_mapping_info: Option<AmsMappingInfo>,
    filament_usage: Vec<EventJobFilamentUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventJobFilamentUsage {
    slot_index: u32,
    source: String,
    ams_id: Option<String>,
    tray_id: Option<String>,
    global_tray_id: Option<u32>,
    external_id: Option<String>,
    filament_id: Option<String>,
    setting_id: Option<String>,
    filament_type: Option<String>,
    color: Option<String>,
    used_mm: Option<String>,
    used_grams: Option<String>,
    confidence: String,
}

impl TryFrom<JobWithArtifact> for JobProjection {
    type Error = RepositoryError;

    fn try_from(value: JobWithArtifact) -> Result<Self, Self::Error> {
        let JobWithArtifact { job, artifact } = value;
        Ok(Self {
            id: job.id.to_string(),
            tenant_id: job.tenant_id.to_string(),
            printer_id: job.printer_id.clone(),
            agent_id: job.agent_id.to_string(),
            artifact_id: job.artifact_id.clone(),
            command_id: job.command_id.to_string(),
            status: job.status.to_string(),
            error: job.error.clone(),
            created_at: job.created_at.clone(),
            updated_at: job.updated_at.clone(),
            print: EventJobPrint::from(job.print.clone()),
            command: EventJobCommand {
                id: job.command_id.to_string(),
                kind: "print_project_file".to_owned(),
                status: job.status.to_string(),
                uploaded_url: job.uploaded_url.clone(),
            },
            artifact: EventJobArtifact::try_from(artifact)?,
            material: EventJobMaterial::try_from(&job)?,
        })
    }
}

impl From<JobPrintState> for EventJobPrint {
    fn from(print: JobPrintState) -> Self {
        Self {
            status: print.status.to_string(),
            printer_state: print.printer_state,
            progress_percent: print.progress_percent,
            remaining_time_minutes: print.remaining_time_minutes,
            current_layer: print.current_layer,
            total_layers: print.total_layers,
            active_file: print.active_file,
            last_progress_percent: print.last_progress_percent,
            last_layer: print.last_layer,
            error: print.error,
            started_at: print.started_at,
            finished_at: print.finished_at,
            updated_at: print.updated_at,
        }
    }
}

impl TryFrom<JobArtifact> for EventJobArtifact {
    type Error = RepositoryError;

    fn try_from(artifact: JobArtifact) -> Result<Self, Self::Error> {
        Ok(Self {
            id: artifact.id,
            tenant_id: artifact.tenant_id.to_string(),
            filename: artifact.filename,
            content_type: artifact.content_type,
            size_bytes: artifact.size_bytes,
            metadata: artifact
                .metadata_json
                .map(|value| serde_json::from_str::<ArtifactMetadata>(&value))
                .transpose()
                .map_err(|error| {
                    RepositoryError::Database(
                        anyhow::Error::new(error).context("invalid persisted artifact metadata"),
                    )
                })?,
            created_at: artifact.created_at,
        })
    }
}

impl EventJobMaterial {
    fn try_from(job: &Job) -> Result<Self, RepositoryError> {
        Ok(Self {
            ams_mapping: parse_mapping(&job.ams_mapping_json, "ams_mapping_json")?,
            ams_mapping2: parse_mapping(&job.ams_mapping2_json, "ams_mapping2_json")?,
            ams_mapping_info: parse_mapping(&job.ams_mapping_info_json, "ams_mapping_info_json")?,
            filament_usage: job
                .filament_usage
                .iter()
                .cloned()
                .map(EventJobFilamentUsage::from)
                .collect(),
        })
    }
}

fn parse_mapping<T>(
    value: &Option<String>,
    field: &'static str,
) -> Result<Option<Vec<T>>, RepositoryError>
where
    T: DeserializeOwned,
{
    value
        .as_deref()
        .map(|value| {
            let parsed = serde_json::from_str::<Vec<T>>(value).map_err(|error| {
                RepositoryError::Database(
                    anyhow::Error::from(error)
                        .context(format!("failed to parse persisted {field}")),
                )
            })?;
            if validate_mapping_len(parsed.len()) {
                Ok(parsed)
            } else {
                Err(RepositoryError::Database(anyhow::anyhow!(
                    "persisted {field} has invalid material mapping shape"
                )))
            }
        })
        .transpose()
}

impl From<JobFilamentUsage> for EventJobFilamentUsage {
    fn from(usage: JobFilamentUsage) -> Self {
        Self {
            slot_index: usage.slot_index,
            source: usage.source,
            ams_id: usage.ams_id,
            tray_id: usage.tray_id,
            global_tray_id: usage.global_tray_id,
            external_id: usage.external_id,
            filament_id: usage.filament_id,
            setting_id: usage.setting_id,
            filament_type: usage.filament_type,
            color: usage.color,
            used_mm: usage.used_mm,
            used_grams: usage.used_grams,
            confidence: usage.confidence,
        }
    }
}
