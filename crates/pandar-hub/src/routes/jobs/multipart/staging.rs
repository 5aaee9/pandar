use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use axum::http::StatusCode;
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{OwnedSemaphorePermit, Semaphore},
};

use pandar_core::TenantId;

use crate::routes::ApiError;

use super::StagedUpload;

const MAX_MULTIPART_TEXT_FIELD_BYTES: usize = 16 * 1024;
const MAX_CONCURRENT_STAGED_UPLOADS: usize = 16;
const MAX_CONCURRENT_STAGED_UPLOADS_PER_TENANT: usize = 2;

#[derive(Debug)]
struct StagingLimits {
    global: Arc<Semaphore>,
    tenants: Mutex<HashMap<TenantId, usize>>,
}

#[derive(Debug)]
pub(super) struct StagingPermit {
    _global: OwnedSemaphorePermit,
    limits: Arc<StagingLimits>,
    tenant_id: TenantId,
}

pub(super) fn acquire_staging_permit(tenant_id: TenantId) -> Result<StagingPermit, ApiError> {
    static LIMITS: OnceLock<Arc<StagingLimits>> = OnceLock::new();
    let limits = Arc::clone(LIMITS.get_or_init(|| {
        Arc::new(StagingLimits {
            global: Arc::new(Semaphore::new(MAX_CONCURRENT_STAGED_UPLOADS)),
            tenants: Mutex::new(HashMap::new()),
        })
    }));
    let global = Arc::clone(&limits.global)
        .try_acquire_owned()
        .map_err(|_| ApiError::new(StatusCode::TOO_MANY_REQUESTS, "artifact_upload_busy"))?;
    {
        let mut tenants = limits.tenants.lock().expect("artifact staging tenants");
        let count = tenants.entry(tenant_id).or_default();
        if *count >= MAX_CONCURRENT_STAGED_UPLOADS_PER_TENANT {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "artifact_upload_busy",
            ));
        }
        *count += 1;
    }
    Ok(StagingPermit {
        _global: global,
        limits,
        tenant_id,
    })
}

impl Drop for StagingPermit {
    fn drop(&mut self) {
        let mut tenants = self
            .limits
            .tenants
            .lock()
            .expect("artifact staging tenants");
        let count = tenants
            .get_mut(&self.tenant_id)
            .expect("artifact staging tenant permit");
        *count -= 1;
        if *count == 0 {
            tenants.remove(&self.tenant_id);
        }
    }
}

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
    let mut path_guard = PartialStagedPath {
        path: path.clone(),
        keep: false,
    };
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path).await.map_err(|err| {
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
    path_guard.keep = true;
    Ok(StagedUpload {
        path,
        filename,
        content_type,
    })
}

struct PartialStagedPath {
    path: std::path::PathBuf,
    keep: bool,
}

impl Drop for PartialStagedPath {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{PartialStagedPath, acquire_staging_permit};

    #[test]
    fn staging_permits_limit_each_tenant_independently() {
        let first_tenant = pandar_core::TenantId::new();
        let second_tenant = pandar_core::TenantId::new();
        let _first = acquire_staging_permit(first_tenant).unwrap();
        let _second = acquire_staging_permit(first_tenant).unwrap();

        assert!(acquire_staging_permit(first_tenant).is_err());
        assert!(acquire_staging_permit(second_tenant).is_ok());
    }

    #[test]
    fn partial_staged_path_is_removed_when_staging_is_cancelled() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("partial-upload");
        std::fs::write(&path, b"partial").unwrap();

        drop(PartialStagedPath {
            path: path.clone(),
            keep: false,
        });

        assert!(!path.exists());
    }
}
