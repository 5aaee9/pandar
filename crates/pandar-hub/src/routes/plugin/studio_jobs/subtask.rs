use serde::Serialize;

use crate::{
    artifacts::metadata::{ArtifactMetadata, FilamentMetadata, PlateMetadata},
    repositories::JobWithArtifact,
    routes::ApiError,
};

#[derive(Debug, Serialize)]
pub(crate) struct StudioSubtaskResponse {
    content: String,
    context: StudioSubtaskContext,
}

#[derive(Debug, Serialize)]
struct StudioSubtaskContent {
    info: StudioSubtaskInfo,
}

#[derive(Debug, Serialize)]
struct StudioSubtaskInfo {
    plate_idx: u32,
}

#[derive(Debug, Serialize)]
struct StudioSubtaskContext {
    plates: Vec<StudioSubtaskPlate>,
}

#[derive(Debug, Serialize)]
struct StudioSubtaskPlate {
    index: u32,
    thumbnail: StudioThumbnail,
    prediction: i32,
    weight: f64,
    filaments: Vec<StudioSubtaskFilament>,
}

#[derive(Debug, Serialize)]
struct StudioThumbnail {
    url: String,
}

#[derive(Debug, Serialize)]
struct StudioSubtaskFilament {
    color: String,
    #[serde(rename = "type")]
    filament_type: String,
    used_g: String,
    used_m: String,
}

pub(super) fn from_job(value: JobWithArtifact) -> Result<StudioSubtaskResponse, ApiError> {
    let metadata = value
        .artifact
        .metadata_json
        .as_deref()
        .ok_or_else(unavailable)
        .and_then(parse_metadata)?;
    let plate = metadata
        .plates
        .iter()
        .find(|plate| plate.plate_id == value.job.plate_index)
        .ok_or_else(unavailable)?;
    let response_plate = response_plate(plate)?;
    let content = serde_json::to_string(&StudioSubtaskContent {
        info: StudioSubtaskInfo {
            plate_idx: value.job.plate_index,
        },
    })
    .map_err(|err| {
        tracing::error!(error = %format!("{err:#}"), "failed to encode Studio subtask content");
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal_server_error",
        )
    })?;
    Ok(StudioSubtaskResponse {
        content,
        context: StudioSubtaskContext {
            plates: vec![response_plate],
        },
    })
}

fn parse_metadata(value: &str) -> Result<ArtifactMetadata, ApiError> {
    serde_json::from_str(value).map_err(|err| {
        tracing::error!(error = %format!("{err:#}"), "invalid persisted Studio artifact metadata");
        unavailable()
    })
}

fn response_plate(value: &PlateMetadata) -> Result<StudioSubtaskPlate, ApiError> {
    let weight = value.filament_weight_grams.ok_or_else(unavailable)?;
    let prediction = i32::try_from(value.estimated_time_seconds.ok_or_else(unavailable)?)
        .map_err(|_| unavailable())?;
    if !weight.is_finite() || weight < 0.0 || !(weight as f32).is_finite() {
        return Err(unavailable());
    }
    Ok(StudioSubtaskPlate {
        index: value.plate_id,
        thumbnail: StudioThumbnail { url: String::new() },
        prediction,
        weight,
        filaments: value
            .filaments
            .iter()
            .map(response_filament)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn response_filament(value: &FilamentMetadata) -> Result<StudioSubtaskFilament, ApiError> {
    Ok(StudioSubtaskFilament {
        color: required_text(value.color.as_deref())?,
        filament_type: required_text(value.filament_type.as_deref())?,
        used_g: studio_float(value.used_grams)?,
        used_m: studio_float(value.used_meters)?,
    })
}

fn required_text(value: Option<&str>) -> Result<String, ApiError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(unavailable)
}

fn studio_float(value: Option<f64>) -> Result<String, ApiError> {
    let value = value.ok_or_else(unavailable)?;
    let rendered = value.to_string();
    if value < 0.0
        || !value.is_finite()
        || !rendered.parse::<f32>().is_ok_and(|value| value.is_finite())
    {
        return Err(unavailable());
    }
    Ok(rendered)
}

fn unavailable() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "studio_task_metadata_unavailable",
    )
}
