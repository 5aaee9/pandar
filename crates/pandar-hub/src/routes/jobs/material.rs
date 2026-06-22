use pandar_core::{Job, JobFilamentUsage};
use serde::Serialize;
use serde_json::Value;

use crate::{repositories::RepositoryError, routes::ApiError};

#[derive(Debug, Clone, Serialize)]
pub struct JobMaterialResponse {
    ams_mapping: Option<Value>,
    ams_mapping2: Option<Value>,
    filament_usage: Vec<JobFilamentUsageResponse>,
}

#[derive(Debug, Clone, Serialize)]
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

pub fn mapping_json(value: Option<Value>, field: &'static str) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let valid = match field {
        "ams_mapping" => valid_ams_mapping(&value),
        "ams_mapping2" => valid_ams_mapping2(&value),
        _ => unreachable!("validated mapping field should be known"),
    };
    if !valid {
        return Err(ApiError::bad_request("invalid_material_mapping"));
    }
    serde_json::to_string(&value)
        .map(Some)
        .map_err(|_| ApiError::bad_request("invalid_material_mapping"))
}

impl JobMaterialResponse {
    pub fn from_job(job: &Job) -> Result<Self, RepositoryError> {
        Ok(Self {
            ams_mapping: parse_persisted_mapping(
                &job.ams_mapping_json,
                "ams_mapping_json",
                valid_ams_mapping,
            )?,
            ams_mapping2: parse_persisted_mapping(
                &job.ams_mapping2_json,
                "ams_mapping2_json",
                valid_ams_mapping2,
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

fn valid_ams_mapping(value: &Value) -> bool {
    let Some(entries) = value.as_array() else {
        return false;
    };
    entries.len() <= 32
        && entries.iter().all(|entry| {
            entry
                .as_i64()
                .and_then(|value| i32::try_from(value).ok())
                .is_some()
        })
}

fn valid_ams_mapping2(value: &Value) -> bool {
    let Some(entries) = value.as_array() else {
        return false;
    };
    entries.len() <= 32
        && entries.iter().all(|entry| {
            let Some(object) = entry.as_object() else {
                return false;
            };
            if object.len() != 2 {
                return false;
            }
            if object
                .get("ams_id")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .is_none()
            {
                return false;
            }
            if object
                .get("slot_id")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .is_none()
            {
                return false;
            }
            true
        })
}

fn parse_persisted_mapping(
    value: &Option<String>,
    field: &'static str,
    valid: fn(&Value) -> bool,
) -> Result<Option<Value>, RepositoryError> {
    value
        .as_deref()
        .map(|value| {
            let parsed = serde_json::from_str::<Value>(value).map_err(|err| {
                RepositoryError::Database(
                    anyhow::Error::from(err).context(format!("failed to parse persisted {field}")),
                )
            })?;
            if valid(&parsed) {
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
