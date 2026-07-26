use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};

use crate::{
    AppState,
    camera_sessions::CameraOpenError,
    repositories::UserRole,
    routes::{ApiError, auth},
};

use super::helpers::parse_printer_id;

pub(crate) async fn printer_camera_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let tenant_id = crate::routes::parse_tenant_id(&tenant_id)?;
    auth::authorize_tenant(&state, &headers, tenant_id, UserRole::Viewer).await?;
    let printer_id = parse_printer_id(&printer_id)?;
    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
    else {
        return Err(ApiError::not_found("printer_not_found"));
    };

    let Some(command_sender) = state
        .sessions()
        .transient_command_sender(tenant_id, printer.agent_id)
        .await
    else {
        return Err(camera_open_error(CameraOpenError::AgentOffline));
    };
    let stream = state
        .camera_sessions()
        .open_stream(
            tenant_id,
            printer.agent_id,
            printer.serial_number,
            command_sender,
        )
        .await
        .map_err(camera_open_error)?;

    Response::builder()
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(stream))
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error"))
}

fn camera_open_error(error: CameraOpenError) -> ApiError {
    match error {
        CameraOpenError::AgentOffline => {
            ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "camera_unavailable")
        }
        CameraOpenError::ChannelClosed | CameraOpenError::ChannelFull => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "camera_channel_unavailable",
        ),
        CameraOpenError::Capacity => {
            ApiError::new(StatusCode::TOO_MANY_REQUESTS, "camera_capacity_exceeded")
        }
    }
}
