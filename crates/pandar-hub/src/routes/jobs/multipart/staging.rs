use axum::http::StatusCode;
use tokio::{fs, io::AsyncWriteExt};

use crate::routes::ApiError;

use super::StagedUpload;

const MAX_MULTIPART_TEXT_FIELD_BYTES: usize = 16 * 1024;

pub(super) async fn read_text_field(
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<String, ApiError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|err| {
        tracing::warn!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            "failed to read multipart text field"
        );
        ApiError::bad_request("artifact_invalid_upload")
    })? {
        if bytes.len().saturating_add(chunk.len()) > MAX_MULTIPART_TEXT_FIELD_BYTES {
            return Err(ApiError::bad_request("artifact_invalid_upload"));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|err| {
        tracing::warn!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            "multipart text field was not UTF-8"
        );
        ApiError::bad_request("artifact_invalid_upload")
    })
}

pub(super) async fn stage_file_field(
    max_artifact_bytes: usize,
    mut field: axum::extract::multipart::Field<'_>,
    filename: Option<String>,
    content_type: Option<String>,
) -> Result<StagedUpload, ApiError> {
    let path = std::env::temp_dir().join(format!("pandar-upload-{}", uuid::Uuid::new_v4()));
    let mut file = fs::File::create(&path).await.map_err(|err| {
        tracing::error!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            "failed to create staged print artifact"
        );
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
    })?;
    let mut size_bytes = 0usize;
    while let Some(chunk) = match field.chunk().await {
        Ok(chunk) => chunk,
        Err(err) => {
            tracing::warn!(
                error = %super::super::redact_artifact_error(&format!("{err:#}")),
                "failed to read staged print artifact field"
            );
            drop(file);
            remove_staged_path(&path).await;
            return Err(ApiError::bad_request("artifact_invalid_upload"));
        }
    } {
        size_bytes = size_bytes.saturating_add(chunk.len());
        if size_bytes > max_artifact_bytes {
            drop(file);
            remove_staged_path(&path).await;
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "artifact_too_large",
            ));
        }
        if let Err(err) = file.write_all(&chunk).await {
            tracing::error!(
                error = %super::super::redact_artifact_error(&format!("{err:#}")),
                "failed to write staged print artifact"
            );
            drop(file);
            remove_staged_path(&path).await;
            return Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_server_error",
            ));
        }
    }
    if size_bytes == 0 {
        drop(file);
        remove_staged_path(&path).await;
        return Err(ApiError::bad_request("artifact_empty"));
    }
    if let Err(err) = file.flush().await {
        tracing::error!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            "failed to flush staged print artifact"
        );
        drop(file);
        remove_staged_path(&path).await;
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_server_error",
        ));
    }
    Ok(StagedUpload {
        path,
        filename,
        content_type,
    })
}

pub(super) async fn cleanup_staged_upload(file: &StagedUpload) {
    remove_staged_path(&file.path).await;
}

async fn remove_staged_path(path: &std::path::Path) {
    if let Err(err) = fs::remove_file(path).await {
        tracing::warn!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            "failed to remove staged print artifact"
        );
    }
}
