use axum::{extract::Multipart, http::StatusCode};
use pandar_core::{Printer, TenantId};
use serde_json::Value;
use tokio::{fs, io::AsyncWriteExt};

use crate::{
    AppState,
    repositories::{AuditActor, CreatePrintJob, JobWithArtifact},
    routes::{ApiError, jobs::material},
};

mod types;

pub(in crate::routes::jobs) use types::{MultipartPrintFields, StagedUpload};

const MAX_MULTIPART_TEXT_FIELD_BYTES: usize = 16 * 1024;

pub(in crate::routes) async fn create_print_job_from_multipart(
    state: &AppState,
    tenant_id: TenantId,
    path_printer_id: Option<String>,
    multipart: Multipart,
    audit_actor: AuditActor,
    log_context: &'static str,
) -> Result<JobWithArtifact, ApiError> {
    let parsed = parse_multipart_print_fields(state, multipart).await?;
    let prepared = prepare_print_job(state, tenant_id, path_printer_id, &parsed).await;
    let (
        printer,
        plate_id,
        ams_mapping_json,
        ams_mapping2_json,
        use_ams,
        flow_cali,
        timelapse,
        filename,
        content_type,
        artifact_metadata,
        upload_file,
    ) = match prepared {
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
    let artifact_id = uuid::Uuid::new_v4().to_string();
    let stored = state
        .artifact_storage()
        .put_artifact(crate::artifacts::StoreArtifactInput {
            tenant_id,
            artifact_id: &artifact_id,
            filename: &filename,
            body: crate::artifacts::ArtifactUploadBody::reader(upload_file),
        })
        .await
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

    let created = state
        .jobs()
        .create_print_job_with_audit(
            CreatePrintJob {
                tenant_id,
                printer_id: printer.id,
                agent_id: printer.agent_id,
                artifact_id,
                artifact_filename: stored.filename,
                artifact_content_type: content_type,
                artifact_size_bytes: stored.size_bytes,
                artifact_storage_path: stored.storage_path.clone(),
                artifact_metadata_json: artifact_metadata.map(|value| value.to_string()),
                plate_id,
                use_ams,
                flow_cali,
                timelapse,
                ams_mapping_json,
                ams_mapping2_json,
            },
            audit_actor,
        )
        .await;

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
    mut multipart: Multipart,
) -> Result<MultipartPrintFields, ApiError> {
    let mut fields = MultipartPrintFields::default();
    loop {
        let Some(field) = (match multipart.next_field().await {
            Ok(field) => field,
            Err(_) => {
                fields.cleanup_staged_uploads().await;
                return Err(ApiError::bad_request("artifact_invalid_upload"));
            }
        }) else {
            break;
        };
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
            Err(_) => {
                fields.cleanup_staged_uploads().await;
                return Err(ApiError::bad_request("artifact_invalid_upload"));
            }
        };
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
            "flow_cali" => parse_bool(&text).map(|value| fields.flow_cali = Some(value)),
            "timelapse" => parse_bool(&text).map(|value| fields.timelapse = Some(value)),
            "ams_mapping" => parse_json_field(&text).map(|value| fields.ams_mapping = Some(value)),
            "ams_mapping2" => {
                parse_json_field(&text).map(|value| fields.ams_mapping2 = Some(value))
            }
            _ => Ok(()),
        };
        if let Err(err) = parsed {
            fields.cleanup_staged_uploads().await;
            return Err(err);
        }
    }

    Ok(fields)
}

async fn read_text_field(
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<String, ApiError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|_| ApiError::bad_request("artifact_invalid_upload"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_MULTIPART_TEXT_FIELD_BYTES {
            return Err(ApiError::bad_request("artifact_invalid_upload"));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| ApiError::bad_request("artifact_invalid_upload"))
}

type PreparedPrintJob = (
    Printer,
    u32,
    Option<String>,
    Option<String>,
    bool,
    bool,
    bool,
    String,
    String,
    Option<serde_json::Value>,
    fs::File,
);

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
    let ams_mapping_json = material::mapping_json(parsed.ams_mapping.clone(), "ams_mapping")?;
    let ams_mapping2_json = material::mapping_json(parsed.ams_mapping2.clone(), "ams_mapping2")?;
    let use_ams = required(parsed.use_ams)?;
    let flow_cali = required(parsed.flow_cali)?;
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
    let artifact_metadata =
        super::metadata_preview::parsed_metadata_json(&filename, &content_type, &file.path).await?;

    Ok((
        printer,
        plate_id,
        ams_mapping_json,
        ams_mapping2_json,
        use_ams,
        flow_cali,
        timelapse,
        filename,
        content_type,
        artifact_metadata,
        upload_file,
    ))
}

async fn stage_file_field(
    max_artifact_bytes: usize,
    mut field: axum::extract::multipart::Field<'_>,
    filename: Option<String>,
    content_type: Option<String>,
) -> Result<StagedUpload, ApiError> {
    let path = std::env::temp_dir().join(format!("pandar-upload-{}", uuid::Uuid::new_v4()));
    let mut file = fs::File::create(&path).await.map_err(|err| {
        tracing::error!(
            error = %super::redact_artifact_error(&format!("{err:#}")),
            "failed to create staged print artifact"
        );
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
    })?;
    let mut size_bytes = 0usize;
    while let Some(chunk) = match field.chunk().await {
        Ok(chunk) => chunk,
        Err(_) => {
            drop(file);
            let _ = fs::remove_file(&path).await;
            return Err(ApiError::bad_request("artifact_invalid_upload"));
        }
    } {
        size_bytes = size_bytes.saturating_add(chunk.len());
        if size_bytes > max_artifact_bytes {
            drop(file);
            let _ = fs::remove_file(&path).await;
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "artifact_too_large",
            ));
        }
        if let Err(err) = file.write_all(&chunk).await {
            tracing::error!(
                error = %super::redact_artifact_error(&format!("{err:#}")),
                "failed to write staged print artifact"
            );
            drop(file);
            let _ = fs::remove_file(&path).await;
            return Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_server_error",
            ));
        }
    }
    if size_bytes == 0 {
        drop(file);
        let _ = fs::remove_file(&path).await;
        return Err(ApiError::bad_request("artifact_empty"));
    }
    if let Err(err) = file.flush().await {
        tracing::error!(
            error = %super::redact_artifact_error(&format!("{err:#}")),
            "failed to flush staged print artifact"
        );
        drop(file);
        let _ = fs::remove_file(&path).await;
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

async fn cleanup_staged_upload(file: &StagedUpload) {
    let _ = fs::remove_file(&file.path).await;
}

fn parse_i64(value: &str) -> Result<i64, ApiError> {
    value
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("bad_request"))
}

fn parse_bool(value: &str) -> Result<bool, ApiError> {
    value
        .parse::<bool>()
        .map_err(|_| ApiError::bad_request("bad_request"))
}

fn parse_json_field(value: &str) -> Result<Value, ApiError> {
    serde_json::from_str(value).map_err(|_| ApiError::bad_request("bad_request"))
}

fn required<T>(value: Option<T>) -> Result<T, ApiError> {
    value.ok_or_else(|| ApiError::bad_request("bad_request"))
}
