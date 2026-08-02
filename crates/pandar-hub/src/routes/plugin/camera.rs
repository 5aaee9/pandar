use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use pandar_core::compatibility::studio_local_camera_supported;

use crate::{
    AppState,
    camera_sessions::CameraOpenError,
    protocol::agent::v1::{AgentCapability, CameraStreamMode},
    routes::{ApiError, auth},
};

pub(crate) async fn stream_camera(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(printer_id): Path<String>,
) -> Result<Response, ApiError> {
    let authenticated = auth::authorize_plugin_studio(&state, &headers).await?;
    let tenant_id = authenticated.token.tenant_id;
    let printer = state
        .printers()
        .get_for_tenant(tenant_id, &printer_id)
        .await?
        .ok_or_else(|| ApiError::not_found("printer_not_found"))?;
    if !studio_local_camera_supported(printer.model.as_deref()) {
        return Err(camera_unavailable());
    }
    let command_sender = state
        .sessions()
        .transient_command_sender_for_capability(
            tenant_id,
            printer.agent_id,
            AgentCapability::StudioLocalCamera,
        )
        .await
        .ok_or_else(camera_unavailable)?;
    let stream = state
        .camera_sessions()
        .open_stream_with_mode(
            tenant_id,
            printer.agent_id,
            printer.serial_number,
            CameraStreamMode::Mjpeg,
            command_sender,
        )
        .await
        .map_err(camera_open_error)?;

    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "multipart/x-mixed-replace; boundary=frame",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(stream))
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error"))
}

fn camera_open_error(error: CameraOpenError) -> ApiError {
    match error {
        CameraOpenError::AgentOffline => camera_unavailable(),
        CameraOpenError::ChannelClosed | CameraOpenError::ChannelFull => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "camera_channel_unavailable",
        ),
        CameraOpenError::Capacity => {
            ApiError::new(StatusCode::TOO_MANY_REQUESTS, "camera_capacity_exceeded")
        }
    }
}

fn camera_unavailable() -> ApiError {
    ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "camera_unavailable")
}
