use super::*;
use crate::Database;
use serde::Deserialize;
use serde_json::Number;
use std::collections::BTreeMap;

mod auth_validation;
mod clear;
mod create;
mod delete;
mod multipart;
mod read;
mod recovery;
mod redaction;

#[derive(Debug, Deserialize)]
struct TenantResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobListResponse {
    jobs: Vec<JobResponse>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct JobResponse {
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
    print: JobPrintResponse,
    command: JobCommandResponse,
    artifact: JobArtifactResponse,
    material: JobMaterialResponse,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct JobPrintResponse {
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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct JobCommandResponse {
    id: String,
    kind: String,
    status: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct JobArtifactResponse {
    id: String,
    tenant_id: String,
    filename: String,
    content_type: String,
    size_bytes: u64,
    metadata: Option<ArtifactPreviewMetadata>,
    created_at: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct JobMaterialResponse {
    ams_mapping: Option<Vec<i32>>,
    ams_mapping2: Option<Vec<AmsMapping2Entry>>,
    ams_mapping_info: Option<Vec<AmsMappingInfoEntry>>,
    filament_usage: Vec<JobFilamentUsageResponse>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AmsMapping2Entry {
    ams_id: i32,
    slot_id: i32,
}

#[derive(Debug, Deserialize, PartialEq)]
struct AmsMappingInfoEntry {
    #[serde(rename = "nozzleId")]
    nozzle_id: i32,
    #[serde(flatten)]
    extra: BTreeMap<String, TestJsonValue>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum TestJsonValue {
    Object(BTreeMap<String, TestJsonValue>),
    Array(Vec<TestJsonValue>),
    String(String),
    Number(Number),
    Bool(bool),
    Null,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
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

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    decode_json(value)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactMetadataPreviewResponse {
    metadata: Option<ArtifactPreviewMetadata>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ArtifactPreviewMetadata {
    source: String,
    display_name: String,
    plate_count: usize,
    default_plate_id: Option<u32>,
    plates: Vec<ArtifactPreviewPlate>,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ArtifactPreviewPlate {
    plate_id: u32,
    name: String,
    object_count: usize,
    objects: Vec<String>,
    estimated_time_seconds: Option<u64>,
    filament_weight_grams: Option<f64>,
    filaments: Vec<ArtifactPreviewFilament>,
    has_thumbnail: bool,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ArtifactPreviewFilament {
    filament_id: Option<String>,
    filament_type: Option<String>,
    color: Option<String>,
    used_grams: Option<f64>,
    used_meters: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct JobRecoveryAuditMetadata {
    source_job_id: String,
    target_job_id: String,
    source_command_id: String,
    target_command_id: String,
    reason: Option<String>,
    #[serde(flatten)]
    _extra: BTreeMap<String, TestJsonValue>,
}
