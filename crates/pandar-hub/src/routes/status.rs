use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{AppState, routes::ApiError};

#[derive(Debug, Serialize)]
pub(in crate::routes) struct HealthResponse {
    status: &'static str,
}

pub(in crate::routes) async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub(in crate::routes) async fn readyz(
    State(state): State<AppState>,
) -> (StatusCode, Json<crate::readiness::ReadinessResponse>) {
    let readiness = crate::readiness::check(&state).await;
    let status = if readiness.status == "ready" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(readiness))
}

pub(in crate::routes) async fn metrics(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let body = crate::metrics_export::prometheus_metrics(&state)
        .await
        .map_err(|err| {
            tracing::error!(error = %format!("{err:#}"), "failed to render metrics");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_server_error")
        })?;
    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
        .into_response())
}
