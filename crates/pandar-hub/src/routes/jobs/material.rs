use pandar_core::{Job, JobFilamentUsage};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{
    material_mapping::{AmsMapping, AmsMapping2, AmsMappingInfo, validate_mapping_len},
    repositories::RepositoryError,
    routes::ApiError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMaterialResponse {
    ams_mapping: Option<AmsMapping>,
    ams_mapping2: Option<AmsMapping2>,
    ams_mapping_info: Option<AmsMappingInfo>,
    filament_usage: Vec<JobFilamentUsageResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobFilamentUsageResponse {
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

pub fn ams_mapping_json(value: Option<AmsMapping>) -> Result<Option<String>, ApiError> {
    typed_mapping_json(value)
}

pub fn ams_mapping2_json(value: Option<AmsMapping2>) -> Result<Option<String>, ApiError> {
    typed_mapping_json(value)
}

pub fn ams_mapping_info_json(value: Option<AmsMappingInfo>) -> Result<Option<String>, ApiError> {
    typed_mapping_json(value)
}

fn typed_mapping_json<T>(value: Option<Vec<T>>) -> Result<Option<String>, ApiError>
where
    T: Serialize,
{
    let Some(value) = value else {
        return Ok(None);
    };
    if !validate_mapping_len(value.len()) {
        return Err(ApiError::bad_request("invalid_material_mapping"));
    }
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|_| ApiError::bad_request("invalid_material_mapping"))
}

impl JobMaterialResponse {
    pub fn from_job(job: &Job) -> Result<Self, RepositoryError> {
        Ok(Self {
            ams_mapping: parse_persisted_mapping(&job.ams_mapping_json, "ams_mapping_json")?,
            ams_mapping2: parse_persisted_mapping(&job.ams_mapping2_json, "ams_mapping2_json")?,
            ams_mapping_info: parse_persisted_mapping(
                &job.ams_mapping_info_json,
                "ams_mapping_info_json",
            )?,
            filament_usage: job
                .filament_usage
                .iter()
                .cloned()
                .map(JobFilamentUsageResponse::from)
                .collect(),
        })
    }
}

fn parse_persisted_mapping<T>(
    value: &Option<String>,
    field: &'static str,
) -> Result<Option<Vec<T>>, RepositoryError>
where
    T: DeserializeOwned,
{
    value
        .as_deref()
        .map(|value| {
            let parsed = serde_json::from_str::<Vec<T>>(value).map_err(|err| {
                RepositoryError::Database(
                    anyhow::Error::from(err).context(format!("failed to parse persisted {field}")),
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

impl From<JobFilamentUsage> for JobFilamentUsageResponse {
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
