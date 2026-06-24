use axum::{extract::Multipart, http::StatusCode};

use crate::{AppState, routes::ApiError};

pub(in crate::routes) async fn preview_artifact_metadata_from_multipart(
    state: &AppState,
    multipart: Multipart,
) -> Result<Option<serde_json::Value>, ApiError> {
    let parsed = super::multipart::parse_multipart_print_fields(state, multipart).await?;
    let result = preview_artifact_metadata(&parsed).await;
    parsed.cleanup_staged_uploads().await;
    result
}

async fn preview_artifact_metadata(
    parsed: &super::multipart::MultipartPrintFields,
) -> Result<Option<serde_json::Value>, ApiError> {
    let file = parsed
        .file
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("artifact_invalid_upload"))?;
    let filename = parsed
        .filename
        .clone()
        .or_else(|| file.filename.clone())
        .ok_or_else(|| ApiError::bad_request("artifact_invalid_upload"))?;
    if filename.trim().is_empty() {
        return Err(ApiError::bad_request("bad_request"));
    }
    let content_type = parsed
        .content_type
        .clone()
        .or_else(|| file.content_type.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    parsed_metadata_json(&filename, &content_type, &file.path).await
}

pub(super) async fn parsed_metadata_json(
    filename: &str,
    content_type: &str,
    path: &std::path::Path,
) -> Result<Option<serde_json::Value>, ApiError> {
    match crate::artifacts::metadata::parse_artifact_metadata(filename, content_type, path) {
        Ok(Some(metadata)) => serde_json::to_value(metadata).map(Some).map_err(|err| {
            tracing::error!(
                error = %format!("{err:#}"),
                "failed to serialize artifact metadata"
            );
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        }),
        Ok(None) => Ok(None),
        Err(err) => {
            tracing::warn!(
                error = %super::redact_artifact_error(&format!("{err:#}")),
                "failed to parse artifact metadata"
            );
            Ok(None)
        }
    }
}
