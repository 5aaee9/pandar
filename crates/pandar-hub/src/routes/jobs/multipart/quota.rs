use axum::http::StatusCode;
use tokio::fs;

use crate::{repositories::ArtifactQuotaLimits, routes::ApiError};

use super::StagedUpload;

pub(super) async fn staged_artifact_size(file: &StagedUpload) -> Result<u64, ApiError> {
    fs::metadata(&file.path)
        .await
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to read staged artifact size");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })
        .map(|metadata| metadata.len())
}

pub(super) fn artifact_quota() -> ArtifactQuotaLimits {
    const DEFAULT_TENANT_ARTIFACT_QUOTA_BYTES: u64 = 1024 * 1024 * 1024;
    const DEFAULT_TENANT_ARTIFACT_QUOTA_COUNT: u64 = 10_000;
    const DEFAULT_GLOBAL_ARTIFACT_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024;
    const DEFAULT_GLOBAL_ARTIFACT_QUOTA_COUNT: u64 = 100_000;
    let tenant_bytes = std::env::var("PANDAR_TENANT_ARTIFACT_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_TENANT_ARTIFACT_QUOTA_BYTES);
    let tenant_count = std::env::var("PANDAR_TENANT_ARTIFACT_QUOTA_COUNT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_TENANT_ARTIFACT_QUOTA_COUNT);
    let global_bytes = std::env::var("PANDAR_GLOBAL_ARTIFACT_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_GLOBAL_ARTIFACT_QUOTA_BYTES);
    let global_count = std::env::var("PANDAR_GLOBAL_ARTIFACT_QUOTA_COUNT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_GLOBAL_ARTIFACT_QUOTA_COUNT);
    ArtifactQuotaLimits {
        tenant_bytes,
        tenant_count,
        global_bytes,
        global_count,
    }
}
