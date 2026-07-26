use axum::{extract::Multipart, http::StatusCode};
use pandar_core::TenantId;
use tokio::fs;

use crate::{
    AppState,
    repositories::{AuditActor, CreatePrintJob, JobWithArtifact},
    routes::{ApiError, jobs::material},
};

use super::metadata_preview::{artifact_metadata_json, parsed_artifact_metadata};
use parsing::{parse_bool, parse_calibration_mode, parse_i64, parse_optional_json_field, required};
use quota::{artifact_quota, staged_artifact_size};
use staging::{acquire_staging_permit, cleanup_staged_upload, read_text_field, stage_file_field};
use types::PreparedPrintJob;

mod parsing;
mod quota;
mod staging;
mod studio;
mod types;

pub(in crate::routes::jobs) use types::{MultipartPrintFields, StagedUpload};

#[derive(Clone, Copy)]
pub(in crate::routes) enum MultipartPrintKind {
    Web,
    Studio,
}

pub(in crate::routes) async fn create_print_job_from_multipart(
    state: &AppState,
    tenant_id: TenantId,
    path_printer_id: Option<String>,
    multipart: Multipart,
    audit_actor: AuditActor,
    log_context: &'static str,
    kind: MultipartPrintKind,
) -> Result<JobWithArtifact, ApiError> {
    let parsed = parse_multipart_print_fields(state, tenant_id, multipart).await?;
    let studio_metadata = match kind {
        MultipartPrintKind::Web => None,
        MultipartPrintKind::Studio => match studio::metadata(&parsed) {
            Ok(metadata) => Some(metadata),
            Err(err) => {
                parsed.cleanup_staged_uploads().await;
                return Err(err);
            }
        },
    };
    let prepared = prepare_print_job(state, tenant_id, path_printer_id, &parsed).await;
    let PreparedPrintJob {
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
    } = match prepared {
        Ok(prepared) => prepared,
        Err(err) => {
            parsed.cleanup_staged_uploads().await;
            return Err(err);
        }
    };
    let file = parsed
        .file
        .as_ref()
        .expect("prepared print job requires staged file");
    let artifact_quota = artifact_quota();
    let upload_bytes = match staged_artifact_size(file).await {
        Ok(upload_bytes) => upload_bytes,
        Err(error) => {
            cleanup_staged_upload(file).await;
            return Err(error);
        }
    };
    let reservation = match state
        .jobs()
        .reserve_artifact_quota(tenant_id, upload_bytes, artifact_quota)
        .await
    {
        Ok(reservation) => reservation,
        Err(error) => {
            cleanup_staged_upload(file).await;
            return Err(error.into());
        }
    };
    let artifact_metadata_json = match artifact_metadata_json(artifact_metadata.as_ref()) {
        Ok(metadata) => metadata,
        Err(error) => {
            cleanup_staged_upload(file).await;
            return Err(error);
        }
    };
    let artifact_id = uuid::Uuid::new_v4().to_string();
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
            error = %super::redact_artifact_error(&format!("{err:#}")),
            context = log_context,
            "failed to write print artifact"
        );
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
    });
    cleanup_staged_upload(file).await;
    let stored = stored?;

    let input = CreatePrintJob {
        tenant_id,
        printer_id: printer.id,
        agent_id: printer.agent_id,
        artifact_id,
        artifact_filename: stored.filename,
        artifact_content_type: content_type,
        artifact_size_bytes: stored.size_bytes,
        artifact_storage_path: stored.storage_path.clone(),
        artifact_metadata_json,
        plate_id,
        use_ams,
        bed_leveling,
        auto_bed_leveling,
        flow_cali,
        auto_flow_cali,
        auto_offset_cali,
        timelapse,
        ams_mapping_json,
        ams_mapping2_json,
        ams_mapping_info_json,
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
            if let Err(cleanup_err) = state
                .artifact_storage()
                .delete_artifact(&stored.storage_key)
                .await
            {
                tracing::warn!(
                    error = %super::redact_artifact_error(&format!("{cleanup_err:#}")),
                    context = log_context,
                    "failed to remove print artifact after repository error"
                );
            }
            Err(err.into())
        }
    }
}

pub(super) async fn parse_multipart_print_fields(
    state: &AppState,
    tenant_id: TenantId,
    multipart: Multipart,
) -> Result<MultipartPrintFields, ApiError> {
    tokio::time::timeout(
        std::time::Duration::from_secs(120),
        parse_multipart_print_fields_inner(state, tenant_id, multipart),
    )
    .await
    .map_err(|_| ApiError::new(StatusCode::REQUEST_TIMEOUT, "artifact_upload_timeout"))?
}

async fn parse_multipart_print_fields_inner(
    state: &AppState,
    tenant_id: TenantId,
    mut multipart: Multipart,
) -> Result<MultipartPrintFields, ApiError> {
    let mut fields = MultipartPrintFields {
        _staging_permit: Some(acquire_staging_permit(tenant_id)?),
        ..MultipartPrintFields::default()
    };
    let mut field_count = 0_usize;
    let mut text_bytes = 0_usize;
    loop {
        let Some(field) = (match multipart.next_field().await {
            Ok(field) => field,
            Err(err) => {
                tracing::warn!(
                    error = %super::redact_artifact_error(&format!("{err:#}")),
                    "failed to read next multipart print field"
                );
                fields.cleanup_staged_uploads().await;
                return Err(ApiError::bad_request("artifact_invalid_upload"));
            }
        }) else {
            break;
        };
        field_count += 1;
        if field_count > 64 {
            fields.cleanup_staged_uploads().await;
            return Err(ApiError::bad_request("artifact_too_many_fields"));
        }
        let name = match field.name() {
            Some(name) => name.to_string(),
            None => {
                fields.cleanup_staged_uploads().await;
                return Err(ApiError::bad_request("artifact_invalid_upload"));
            }
        };
        if name == "file" || name == "artifact" {
            if fields.file.is_some() {
                fields.cleanup_staged_uploads().await;
                return Err(ApiError::bad_request("artifact_invalid_upload"));
            }
            let filename = field.file_name().map(ToOwned::to_owned);
            let content_type = field.content_type().map(ToString::to_string);
            let staged = match stage_file_field(
                state.artifact_storage().max_artifact_bytes(),
                field,
                filename,
                content_type,
            )
            .await
            {
                Ok(staged) => staged,
                Err(err) => {
                    fields.cleanup_staged_uploads().await;
                    return Err(err);
                }
            };
            fields.file = Some(staged);
            continue;
        }

        let text = match read_text_field(field).await {
            Ok(text) => text,
            Err(err) => {
                fields.cleanup_staged_uploads().await;
                return Err(err);
            }
        };
        text_bytes = text_bytes.saturating_add(text.len());
        if text_bytes > 256 * 1024 {
            fields.cleanup_staged_uploads().await;
            return Err(ApiError::bad_request("artifact_text_fields_too_large"));
        }
        let parsed = match name.as_str() {
            "printer_id" => {
                fields.printer_id = Some(text);
                Ok(())
            }
            "filename" => {
                fields.filename = Some(text);
                Ok(())
            }
            "content_type" => {
                fields.content_type = Some(text);
                Ok(())
            }
            "plate_id" => parse_i64(&text).map(|value| fields.plate_id = Some(value)),
            "use_ams" => parse_bool(&text).map(|value| fields.use_ams = Some(value)),
            "bed_leveling" => parse_bool(&text).map(|value| fields.bed_leveling = Some(value)),
            "auto_bed_leveling" => {
                parse_calibration_mode(&text).map(|value| fields.auto_bed_leveling = Some(value))
            }
            "flow_cali" => parse_bool(&text).map(|value| fields.flow_cali = Some(value)),
            "auto_flow_cali" => {
                parse_calibration_mode(&text).map(|value| fields.auto_flow_cali = Some(value))
            }
            "auto_offset_cali" => {
                parse_calibration_mode(&text).map(|value| fields.auto_offset_cali = Some(value))
            }
            "timelapse" => parse_bool(&text).map(|value| fields.timelapse = Some(value)),
            "ams_mapping" => {
                parse_optional_json_field(&text).map(|value| fields.ams_mapping = value)
            }
            "ams_mapping2" => {
                parse_optional_json_field(&text).map(|value| fields.ams_mapping2 = value)
            }
            "ams_mapping_info" => {
                parse_optional_json_field(&text).map(|value| fields.ams_mapping_info = value)
            }
            _ => studio::parse_field(&mut fields, &name, &text),
        };
        if let Err(err) = parsed {
            fields.cleanup_staged_uploads().await;
            return Err(err);
        }
    }

    Ok(fields)
}

async fn prepare_print_job(
    state: &AppState,
    tenant_id: TenantId,
    path_printer_id: Option<String>,
    parsed: &MultipartPrintFields,
) -> Result<PreparedPrintJob, ApiError> {
    let printer_id = path_printer_id
        .or_else(|| parsed.printer_id.clone())
        .ok_or_else(|| ApiError::bad_request("invalid_printer_id"))?;
    super::parse_printer_id(&printer_id)?;
    let plate_id = super::validated_plate_id(required(parsed.plate_id)?)?;
    let ams_mapping_json = material::ams_mapping_json(parsed.ams_mapping.clone())?;
    let ams_mapping2_json = material::ams_mapping2_json(parsed.ams_mapping2.clone())?;
    let ams_mapping_info_json = material::ams_mapping_info_json(parsed.ams_mapping_info.clone())?;
    let use_ams = required(parsed.use_ams)?;
    let bed_leveling = required(parsed.bed_leveling)?;
    let auto_bed_leveling = required(parsed.auto_bed_leveling)?;
    let flow_cali = required(parsed.flow_cali)?;
    let auto_flow_cali = required(parsed.auto_flow_cali)?;
    let auto_offset_cali = required(parsed.auto_offset_cali)?;
    let timelapse = required(parsed.timelapse)?;
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
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await?
        .ok_or_else(|| ApiError::not_found("printer_not_found"))?;
    let upload_file = fs::File::open(&file.path).await.map_err(|err| {
        tracing::error!(
            error = %super::redact_artifact_error(&format!("{err:#}")),
            "failed to open staged print artifact"
        );
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
    })?;
    let artifact_metadata = parsed_artifact_metadata(&filename, &content_type, &file.path).await?;

    Ok(PreparedPrintJob {
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
    })
}
