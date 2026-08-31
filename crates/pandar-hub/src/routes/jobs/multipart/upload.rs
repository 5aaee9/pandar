use axum::http::StatusCode;
use pandar_core::{StudioPrintMetadata, TenantId};

use crate::{
    AppState,
    repositories::{AuditActor, CreatePrintJob, JobWithArtifact},
    routes::ApiError,
};

use super::{
    StagedUpload,
    quota::{artifact_quota, staged_artifact_size},
    staging::cleanup_staged_upload,
};

pub(super) async fn persist(
    state: &AppState,
    tenant_id: TenantId,
    file: &StagedUpload,
    prepared: super::types::PreparedPrintJob,
    studio_metadata: Option<StudioPrintMetadata>,
    audit_actor: AuditActor,
    log_context: &'static str,
) -> Result<JobWithArtifact, ApiError> {
    let super::types::PreparedPrintJob {
        printer,
        plate_id,
        ams_mapping_json,
        ams_mapping2_json,
        ams_mapping_info_json,
        use_ams,
        bed_leveling,
        auto_bed_leveling,
        flow_cali,
        auto_flow_cali,
        auto_offset_cali,
        timelapse,
        filename,
        content_type,
        artifact_metadata,
        upload_file,
    } = prepared;
    let upload_bytes = match staged_artifact_size(file).await {
        Ok(upload_bytes) => upload_bytes,
        Err(error) => {
            cleanup_staged_upload(file).await;
            return Err(error);
        }
    };
    let artifact_metadata_json =
        match super::super::metadata_preview::artifact_metadata_json(artifact_metadata.as_ref()) {
            Ok(metadata) => metadata,
            Err(error) => {
                cleanup_staged_upload(file).await;
                return Err(error);
            }
        };
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let storage_path = state
        .artifact_storage()
        .storage_key(tenant_id, &artifact_id, &filename);
    let reservation = match state
        .jobs()
        .reserve_artifact_quota(
            tenant_id,
            artifact_id.clone(),
            storage_path,
            upload_bytes,
            artifact_quota(),
        )
        .await
    {
        Ok(reservation) => reservation,
        Err(error) => {
            cleanup_staged_upload(file).await;
            return Err(error.into());
        }
    };
    let stored = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        state
            .artifact_storage()
            .put_artifact(crate::artifacts::StoreArtifactInput {
                tenant_id,
                artifact_id: &artifact_id,
                filename: &filename,
                body: crate::artifacts::ArtifactUploadBody::reader(upload_file),
            }),
    )
    .await
    .map_err(|err| anyhow::Error::new(err).context("print artifact storage timed out"))
    .and_then(|stored| stored)
    .map_err(|err| {
        tracing::error!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            context = log_context,
            "failed to write print artifact"
        );
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
    });
    cleanup_staged_upload(file).await;
    let stored = match stored {
        Ok(stored) => stored,
        Err(error) => {
            if let Err(err) = reservation.release().await {
                tracing::error!(
                    error = %super::super::redact_artifact_error(&format!("{err:#}")),
                    context = log_context,
                    "failed to release artifact quota reservation after upload failure"
                );
            }
            return Err(error);
        }
    };

    let input = CreatePrintJob {
        tenant_id,
        printer_id: printer.id,
        agent_id: printer.agent_id,
        artifact: crate::repositories::PrintArtifactInput {
            id: artifact_id,
            filename: stored.filename,
            content_type,
            size_bytes: stored.size_bytes,
            storage_path: stored.storage_key,
            metadata_json: artifact_metadata_json,
        },
        options: crate::repositories::PrintExecutionOptions {
            plate_id,
            use_ams,
            auto_bed_leveling,
            bed_leveling,
            flow_cali,
            auto_flow_cali,
            auto_offset_cali,
            timelapse,
            ams_mapping_json,
            ams_mapping2_json,
            ams_mapping_info_json,
        },
    };
    let created = match studio_metadata {
        Some(metadata) => {
            reservation
                .create_studio_print_job_with_audit(input, metadata, audit_actor)
                .await
        }
        None => {
            reservation
                .create_print_job_with_audit(input, audit_actor)
                .await
        }
    };
    match created {
        Ok(created) => Ok(created),
        Err(err) => {
            if let Err(release_err) = reservation.release().await {
                tracing::error!(
                    error = %super::super::redact_artifact_error(&format!("{release_err:#}")),
                    context = log_context,
                    "failed to release artifact quota reservation after repository error"
                );
            }
            drain_deletions(state, log_context, "repository error").await;
            Err(err.into())
        }
    }
}

async fn drain_deletions(state: &AppState, log_context: &'static str, reason: &'static str) {
    if let Err(err) =
        crate::artifacts::lifecycle::drain_deletions(state.database(), state.artifact_storage())
            .await
    {
        tracing::warn!(
            error = %super::super::redact_artifact_error(&format!("{err:#}")),
            context = log_context,
            reason,
            "failed to drain artifact deletion queue"
        );
    }
}
